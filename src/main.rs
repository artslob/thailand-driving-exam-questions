use anyhow::{bail, Context, Result};
use itertools::Itertools;
use minidom::{Element, NSChoice, Node};
use regex::Regex;
use serde::Serialize;
use std::borrow::Cow;
use tinytemplate::TinyTemplate;

static QUESTION_CLASS: &str = "has-luminous-vivid-orange-color has-text-color";
static IMAGE_CLASS: &str = "wp-block-image";

static TEMPLATE: &str = include_str!("../template.html");

fn main() -> Result<()> {
    let file = std::fs::read_to_string("pages/7.html")?;
    let content = file.replace("&nbsp;", " ").replace("<br>", "<br/>");
    let content = fix_img_tags(&content)?;
    let prefixes = (None, String::new());
    let root = Element::from_reader_with_prefixes(content.as_bytes(), prefixes)?;
    let questions = parse_questions(&root)?;
    dbg!(&questions);

    let mut tt = TinyTemplate::new();
    tt.add_template("template", TEMPLATE)?;

    let render_context = RenderContext {
        title: questions[0].title.clone(),
        img_src: questions[0].img_src.clone(),
        answer_choices: questions[0].answer_choices.clone(),
        total: questions.len(),
    };
    let html = tt.render("template", &render_context)?;
    std::fs::write("output/1.html", html)?;

    Ok(())
}

fn parse_questions(root: &Element) -> Result<Vec<Question>> {
    let mut questions = vec![];

    let mut element_iter = root.children();
    while let Some(next) = element_iter.next() {
        let is_question = next.name() == "p" && next.attr("class") == Some(QUESTION_CLASS);
        if !is_question {
            continue;
        }
        let question_title = next
            .children()
            .filter(|el| el.name() == "strong")
            .flat_map(|el| el.texts())
            .join(" ");
        let question_title = normalize_question_title(&question_title)?;

        let next = element_iter
            .next()
            .context("no html elements after question title")?;

        let (next, img_src) = if next.name() == "div" && next.attr("class") == Some(IMAGE_CLASS) {
            let img = next
                .get_child("figure", NSChoice::Any)
                .context("image div does not have inner figure tag")?
                .get_child("img", NSChoice::Any)
                .context("figure tag does not have inner img tag")?;
            let src = img.attr("src").context("img tag does not have src attr")?;
            (
                element_iter
                    .next()
                    .context("expected to have elements after image")?,
                Some(src.to_owned()),
            )
        } else {
            (next, None)
        };

        if next.name() != "p" {
            bail!("expected to have questions p tag")
        }
        let answer_choices = next
            .nodes()
            .flat_map(|node| match node {
                Node::Element(element) => (element.name() == "strong").then(|| AnswerChoice {
                    text: normalize_answer_text(element.texts().join(" ")).unwrap(),
                    is_answer: true,
                }),
                Node::Text(text) => Some(AnswerChoice {
                    text: normalize_answer_text(text).unwrap(),
                    is_answer: false,
                }),
            })
            .collect_vec();

        questions.push(Question {
            title: question_title,
            img_src,
            answer_choices,
        })
    }

    Ok(questions)
}

#[derive(Debug, Serialize)]
struct RenderContext {
    title: String,
    img_src: Option<String>,
    answer_choices: Vec<AnswerChoice>,
    /// total number of questions
    total: usize,
}

#[derive(Debug)]
struct Question {
    title: String,
    img_src: Option<String>,
    answer_choices: Vec<AnswerChoice>,
}

#[derive(Debug, Serialize, Clone)]
struct AnswerChoice {
    text: String,
    is_answer: bool,
}

fn normalize_question_title(title: &str) -> Result<String> {
    let regex = Regex::new(r#"^[\d.]+"#)?;

    Ok(normalize_string(regex.replace(title.trim(), "")))
}

fn normalize_answer_text(text: impl Into<String>) -> Result<String> {
    let regex = Regex::new(r#"^[[:alpha:]]\."#)?;
    Ok(normalize_string(regex.replace(text.into().trim(), "")))
}

fn normalize_string(s: impl Into<String>) -> String {
    s.into()
        .replace('\n', " ")
        .split_whitespace()
        .join(" ")
        .trim()
        .to_owned()
}

fn fix_img_tags(input: &str) -> Result<Cow<str>> {
    let regex = Regex::new("<img (?P<body>(?s:.)*?)/?>")?;
    Ok(regex.replace_all(input, "<img $body/>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_img() -> Result<()> {
        let input = r##"
        <img decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/02/1-3-1-c6d1.jpg.webp"
            class="wp-image-6962" width="300" height="211">
        <p>A. Be extra careful. Then stop the car.</p>
        <img decoding="async" class="wp-image-6962" width="300" height="211">
        <img decoding="async" class="wp-image-6962" width="555" height="100"/>
        <img decoding="async" class="wp-image-3232" width="666" height="555">
        <figure></figure>
        <figure class="aligncenter size-large">
          <img decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/02/1-3-1-c6d1.jpg.webp"
            alt="Thai Driving License Exam Test Questions and Answers in 2020" class="wp-image-6962" width="300" height="211"/>
        </figure>
        "##;
        let result = fix_img_tags(input)?;
        let expected = r##"
        <img decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/02/1-3-1-c6d1.jpg.webp"
            class="wp-image-6962" width="300" height="211"/>
        <p>A. Be extra careful. Then stop the car.</p>
        <img decoding="async" class="wp-image-6962" width="300" height="211"/>
        <img decoding="async" class="wp-image-6962" width="555" height="100"/>
        <img decoding="async" class="wp-image-3232" width="666" height="555"/>
        <figure></figure>
        <figure class="aligncenter size-large">
          <img decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/02/1-3-1-c6d1.jpg.webp"
            alt="Thai Driving License Exam Test Questions and Answers in 2020" class="wp-image-6962" width="300" height="211"/>
        </figure>
        "##;
        assert_eq!(result, expected);
        Ok(())
    }

    #[test]
    fn test_normalize_string() {
        let input = "  this\n   is test\n string\n";
        let result = normalize_string(input);
        assert_eq!(result, "this is test string");
    }

    #[test]
    fn test_normalize_question_title() -> Result<()> {
        let input = "15.1 this\n   is test\n question\n";
        let result = normalize_question_title(input)?;
        assert_eq!(result, "this is test question");
        Ok(())
    }

    #[test]
    fn test_normalize_answer_text() -> Result<()> {
        assert_eq!(
            normalize_answer_text("D. this\n   is test\n answer\n")?,
            "this is test answer"
        );
        assert_eq!(
            normalize_answer_text("b.this\n   is test\n answer\n")?,
            "this is test answer"
        );
        Ok(())
    }
}

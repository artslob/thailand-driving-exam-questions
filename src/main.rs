use anyhow::{anyhow, Context, Result};
use itertools::Itertools;
use lazy_static::lazy_static;
use minidom::{Element, NSChoice, Node};
use regex::Regex;
use serde::Serialize;
use std::borrow::Cow;
use std::path::Path;
use tinytemplate::TinyTemplate;

static QUESTION_CLASS: &str = "has-luminous-vivid-orange-color has-text-color";
static IMAGE_CLASS: &str = "wp-block-image";

static TEMPLATE: &str = include_str!("../template.html");

static PAGES_DIR: &str = "pages";
static OUTPUT_DIR: &str = "output";

fn main() -> Result<()> {
    let file_iter = std::fs::read_dir(PAGES_DIR)?;

    let questions: Vec<_> = itertools::process_results(file_iter, |entry_iter| {
        entry_iter
            .sorted_by_key(|entry| entry.file_name())
            .map(|entry| -> Result<_> {
                if entry.file_name() != "01.html" && entry.file_name() != "02.html" {
                    return Ok(vec![]);
                }
                let content = std::fs::read_to_string(entry.path())?;
                let content = content.replace("&nbsp;", " ").replace("<br>", "<br/>");
                let content = fix_img_tags(&content);
                let prefixes = (None, String::new());
                let root = Element::from_reader_with_prefixes(content.as_bytes(), prefixes)?;
                parse_questions(&root)
            })
            .flatten_ok()
            .try_collect()
    })??;

    let mut tt = TinyTemplate::new();
    tt.add_template("template", TEMPLATE)?;

    let total_count = questions.len();

    questions
        .into_iter()
        .enumerate()
        .try_for_each(|(i, question)| -> Result<_> {
            let index = i + 1;
            let img_src = question
                .img_src
                .as_ref()
                .map(download_image)
                .transpose()
                .context(anyhow!("could not download image: {:?}", question.img_src))?;
            let render_context = RenderContext {
                title: question.title,
                img_src,
                answer_choices: question.answer_choices,
                total: total_count,
                previous_index: (index - 1 > 0).then_some(index - 1),
                next_index: (index < total_count).then_some(index + 1),
            };
            let html = tt.render("template", &render_context)?;
            let output_path = format!("{OUTPUT_DIR}/{index}.html");
            std::fs::write(output_path, html)?;
            Ok(())
        })?;

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
            .nodes()
            .map(|node| match node {
                Node::Element(el) => el.texts().join(" "),
                Node::Text(text) => text.to_owned(),
            })
            .join(" ");
        let question_title = normalize_question_title(&question_title);

        let next = element_iter
            .by_ref()
            .find(|el| {
                let is_image = is_image_element(el);
                let is_answer_choice = el.name() == "p";
                is_image || is_answer_choice
            })
            .context("no html elements after question title")?;

        let (img_src, next) = if is_image_element(next) {
            let src = next
                .get_child("figure", NSChoice::Any)
                .context("image div does not have inner figure tag")?
                .get_child("img", NSChoice::Any)
                .context("figure tag does not have inner img tag")?
                .attr("src")
                .context("img tag does not have src attr")?;

            let question_element = element_iter
                .by_ref()
                .find(|el| {
                    let is_answer_choice = el.name() == "p";
                    is_answer_choice
                })
                .context("expected to have questions p tag")?;
            (Some(src.to_owned()), question_element)
        } else {
            (None, next)
        };

        let answer_choices = next
            .nodes()
            .flat_map(|node| match node {
                Node::Element(element) => (element.name() == "strong").then(|| AnswerChoice {
                    text: normalize_answer_text(element.texts().join(" ")),
                    is_answer: true,
                }),
                Node::Text(text) => Some(AnswerChoice {
                    text: normalize_answer_text(text),
                    is_answer: false,
                }),
            })
            .filter(|choice| !choice.text.is_empty())
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
    /// number of previous question
    previous_index: Option<usize>,
    /// number of next question
    next_index: Option<usize>,
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

fn is_image_element(e: &Element) -> bool {
    e.name() == "div" && e.attr("class") == Some(IMAGE_CLASS)
}

fn normalize_question_title(title: &str) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"^[\d.]+"#).unwrap();
    }
    normalize_string(RE.replace(title.trim(), ""))
}

fn normalize_answer_text(text: impl Into<String>) -> String {
    lazy_static! {
        static ref RE: Regex = Regex::new(r#"^[[:alpha:]]\."#).unwrap();
    }
    normalize_string(RE.replace(text.into().trim(), ""))
}

fn normalize_string(s: impl Into<String>) -> String {
    s.into()
        .replace('\n', " ")
        .split_whitespace()
        .join(" ")
        .trim()
        .to_owned()
}

fn download_image(url: impl Into<String>) -> Result<String> {
    let url = url.into();
    let name = extract_image_name(&url)?;
    let img_src = format!("images/{name}");
    let output_path = format!("{OUTPUT_DIR}/{img_src}");

    if !Path::new(&output_path).try_exists()? {
        let bytes = reqwest::blocking::get(&url)?.error_for_status()?.bytes()?;
        std::fs::write(output_path, bytes)?;
    }

    Ok(img_src)
}

fn extract_image_name(url: impl Into<String>) -> Result<String> {
    let url = url.into();
    url.rsplit('/')
        .map(ToOwned::to_owned)
        .next()
        .context(anyhow!("url does not have /: {url}"))
}

fn fix_img_tags(input: &str) -> Cow<str> {
    lazy_static! {
        static ref RE: Regex = Regex::new("<img(?P<body>(?s:.)*?)/?>").unwrap();
    }
    RE.replace_all(input, "<img$body/>")
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
        <figure class="wp-block-ta-image wp-block-image"><a class="thirstylinkimg" rel="nofollow noindex" title="prakan-motorbike-insurance"
                                                            href="https://move2thailand.com/recommends/motorbike-insurance/" data-shortcode="true"><img
                decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand.jpg.webp" alt=""
                class="wp-image-14258"
                srcset="https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand.jpg.webp 800w, https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand-300x60.jpg.webp 300w, https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand-768x154.jpg.webp 768w"
                sizes="(max-width: 800px) 100vw, 800px" width="800" height="160"></a>
            <figcaption></figcaption>
        </figure>
        "##;
        let result = fix_img_tags(input);
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
        <figure class="wp-block-ta-image wp-block-image"><a class="thirstylinkimg" rel="nofollow noindex" title="prakan-motorbike-insurance"
                                                            href="https://move2thailand.com/recommends/motorbike-insurance/" data-shortcode="true"><img
                decoding="async" src="https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand.jpg.webp" alt=""
                class="wp-image-14258"
                srcset="https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand.jpg.webp 800w, https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand-300x60.jpg.webp 300w, https://move2thailand.com/wp-content/uploads/2020/10/motorbike-insurance-thailand-768x154.jpg.webp 768w"
                sizes="(max-width: 800px) 100vw, 800px" width="800" height="160"/></a>
            <figcaption></figcaption>
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
        let result = normalize_question_title(input);
        assert_eq!(result, "this is test question");
        Ok(())
    }

    #[test]
    fn test_normalize_answer_text() -> Result<()> {
        assert_eq!(
            normalize_answer_text("D. this\n   is test\n answer\n"),
            "this is test answer"
        );
        assert_eq!(
            normalize_answer_text("b.this\n   is test\n answer\n"),
            "this is test answer"
        );
        Ok(())
    }

    #[test]
    fn test_extract_image_name() -> Result<()> {
        assert_eq!(
            extract_image_name(
                "https://move2thailand.com/wp-content/uploads/2020/02/1-3-1-c6d1.jpg.webp"
            )?,
            "1-3-1-c6d1.jpg.webp"
        );
        Ok(())
    }
}

use anyhow::Result;
use minidom::Element;
use regex::Regex;
use std::borrow::Cow;

fn main() -> Result<()> {
    let file = std::fs::read_to_string("pages/7.html")?;
    let content = file.replace("&nbsp;", " ").replace("<br>", "<br/>");
    let content = fix_img_tags(&content)?;
    let prefixes = (None, String::new());
    let _root = Element::from_reader_with_prefixes(content.as_bytes(), prefixes)?;
    Ok(())
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
}

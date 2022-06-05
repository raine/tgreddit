use crate::reddit;

fn format_html_anchor(href: &str, text: &str) -> String {
    format!(
        r#"<a href="{href}">{}</a>"#,
        html_escape::encode_text(&text)
    )
}

pub fn format_video_caption_html(post: &reddit::Post) -> String {
    let title = &post.title;
    let subreddit_link = format_html_anchor(
        &reddit::format_subreddit_url(&post.subreddit),
        &format!("/r/{}", post.subreddit),
    );
    let comments_link = format_html_anchor(&post.format_permalink_url(), "comments");
    format!("{title}\n{subreddit_link} [{comments_link}]")
}

#[cfg(test)]
mod tests {
    use super::format_html_anchor;

    #[test]
    fn test_format_html_anchor() {
        assert_eq!(
            format_html_anchor("https://example.com", "<hello></world>"),
            r#"<a href="https://example.com">&lt;hello&gt;&lt;/world&gt;</a>"#
        )
    }
}

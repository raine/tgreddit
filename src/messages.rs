use crate::reddit;
use crate::*;
use itertools::Itertools;

fn escape(html: &str) -> String {
    html.replace('<', "&lt;").replace('>', "&gt;")
}

fn format_html_anchor(href: &str, text: &str) -> String {
    format!(r#"<a href="{href}">{}</a>"#, escape(text))
}

fn format_subreddit_link(subreddit: &str) -> String {
    format_html_anchor(
        &reddit::format_subreddit_url(subreddit),
        &format!("/r/{}", &subreddit),
    )
}

pub fn format_meta_html(post: &reddit::Post) -> String {
    let subreddit_link = format_subreddit_link(&post.subreddit);
    let comments_link = format_html_anchor(&post.format_permalink_url(), "comments");
    let old_comments_link = format_html_anchor(&post.format_old_permalink_url(), "old");
    format!("{subreddit_link} [{comments_link}, {old_comments_link}]")
}

pub fn format_media_caption_html(post: &reddit::Post) -> String {
    let title = &post.title;
    let meta = format_meta_html(post);
    format!("{title}\n{meta}")
}

pub fn format_link_message_html(post: &reddit::Post) -> String {
    let title = format_html_anchor(&post.url, &post.title);
    let meta = format_meta_html(post);
    format!("{title}\n{meta}")
}

pub fn format_self_message_html(post: &reddit::Post) -> String {
    format_media_caption_html(post)
}

pub fn format_subscription_list(post: &[Subscription]) -> String {
    post.iter().map(|sub| sub.subreddit.to_owned()).join("\n")
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

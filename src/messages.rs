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
    fn format_subscription(sub: &Subscription) -> String {
        let mut args = vec![];
        if let Some(time) = sub.time {
            args.push(format!("time={}", time));
        }
        if let Some(limit) = sub.limit {
            args.push(format!("limit={}", limit));
        }
        if let Some(filter) = sub.filter {
            args.push(format!("filter={}", filter));
        }

        let args_str = if !args.is_empty() {
            format!("({})", args.join(", "))
        } else {
            "".to_string()
        };

        [sub.subreddit.to_owned(), args_str]
            .join(" ")
            .trim_end()
            .to_string()
    }

    if post.is_empty() {
        "No subscriptions".to_owned()
    } else {
        post.iter().map(format_subscription).join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_html_anchor() {
        assert_eq!(
            format_html_anchor("https://example.com", "<hello></world>"),
            r#"<a href="https://example.com">&lt;hello&gt;&lt;/world&gt;</a>"#
        )
    }

    #[test]
    fn test_format_subscription_list() {
        assert_eq!(
            format_subscription_list(&[
                Subscription {
                    chat_id: 1,
                    subreddit: "foo".to_owned(),
                    limit: None,
                    time: None,
                    filter: None,
                },
                Subscription {
                    chat_id: 1,
                    subreddit: "bar".to_owned(),
                    limit: Some(1),
                    time: Some(TopPostsTimePeriod::Week),
                    filter: None,
                },
            ]),
            "foo\nbar (time=week, limit=1)"
        )
    }
}

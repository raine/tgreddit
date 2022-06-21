use crate::{config::Config, reddit::Post};
use anyhow::{Context, Result};
use rusqlite::{named_params, Connection};
use rusqlite_migration::{Migrations, M};
use std::path::Path;

const MIGRATIONS: &[&str] = &[r"
    create table post(
        post_id     text not null,
        chat_id     integer not null,
        subreddit   text not null,
        seen_at     text not null,
        primary key (post_id, chat_id)
    ) strict
"];

#[derive(Debug)]
pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn open(config: &Config) -> Result<Self> {
        let conn = Self::get_conn(&config.db_path).context("error connecting to database")?;
        conn.pragma_update(None, "foreign_keys", &"ON")?;
        let mut db = Database { conn };
        db.migrate().context("migration failed")?;
        Ok(db)
    }

    #[cfg(test)]
    fn get_conn(_db_path: &Path) -> Result<Connection, rusqlite::Error> {
        Connection::open_in_memory()
    }

    #[cfg(not(test))]
    fn get_conn(db_path: &Path) -> Result<Connection, rusqlite::Error> {
        Connection::open(db_path)
    }

    fn migrate(&mut self) -> Result<(), rusqlite_migration::Error> {
        let migrations = MIGRATIONS.iter().map(|e| M::up(*e)).collect();
        Migrations::new(migrations).to_latest(&mut self.conn)
    }

    pub fn mark_post_seen(&self, chat_id: i64, post: &Post) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "
            insert into post (post_id, chat_id, subreddit, seen_at)
            values (:post_id, :chat_id, :subreddit, :seen_at)
            ",
        )?;
        stmt.execute(named_params! {
            ":post_id": post.id,
            ":chat_id": chat_id,
            ":subreddit": &post.subreddit,
            ":seen_at": chrono::Utc::now()
        })
        .context("could not mark post seen")?;
        Ok(())
    }

    pub fn is_post_seen(&self, chat_id: i64, post: &Post) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "
            select exists(
                select 1 
                  from post
                 where post_id = :post_id and chat_id = :chat_id
            );
            ",
        )?;

        stmt.query_row(
            named_params! {
                ":post_id": post.id,
                ":chat_id": chat_id
            },
            |row| row.get(0),
        )
        .map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reddit::PostType;

    #[test]
    fn test_db() {
        let config = Config::default();
        let db = Database::open(&config).unwrap();
        let post = Post {
            id: "v6nu75".into(),
            created: 1654581100.0,
            post_hint: Some("link".into()),
            subreddit: "absoluteunit".into(),
            title: "Tipping a cow to trim its hooves".into(),
            is_self: false,
            is_video: false,
            ups: 469,
            permalink: "/r/absoluteunit/comments/v6nu75/tipping_a_cow_to_trim_its_hooves/".into(),
            url: "https://i.imgur.com/Zt6f5mB.gifv".into(),
            post_type: PostType::Video,
            crosspost_parent_list: None,
        };

        db.mark_post_seen(1, &post).unwrap();
        assert!(db.is_post_seen(1, &post).unwrap());
    }
}

//! Storage for messages carrying a "Translate" button and for the shared
//! per-user language preference used by translated menus.

use sqlx::FromRow;

use super::Db;

/// Sentinel used by the JavaScript bot for DM/global language preferences.
pub const DM_GUILD_ID: &str = "__dm__";

/// Prefer a real guild id; fall back to the stable DM sentinel so the NOT NULL
/// constraint in `user_language_prefs` is never violated.
pub fn resolve_lang_guild_id(guild_id: Option<u64>) -> String {
    match guild_id {
        Some(id) if id != 0 => id.to_string(),
        _ => DM_GUILD_ID.to_string(),
    }
}

/// Guild preference → DM/global preference → English. Mirrors `getUserLang`.
pub async fn get_user_lang(db: &Db, guild_id: Option<u64>, user_id: u64) -> String {
    let gid = resolve_lang_guild_id(guild_id);
    let uid = user_id.to_string();
    let row: Option<(String,)> =
        sqlx::query_as("SELECT lang FROM user_language_prefs WHERE guild_id = ? AND user_id = ?")
            .bind(&gid)
            .bind(&uid)
            .fetch_optional(&db.pool)
            .await
            .ok()
            .flatten();
    if let Some((lang,)) = row {
        return lang;
    }
    if gid != DM_GUILD_ID {
        let fallback: Option<(String,)> = sqlx::query_as(
            "SELECT lang FROM user_language_prefs WHERE guild_id = ? AND user_id = ?",
        )
        .bind(DM_GUILD_ID)
        .bind(&uid)
        .fetch_optional(&db.pool)
        .await
        .ok()
        .flatten();
        if let Some((lang,)) = fallback {
            return lang;
        }
    }
    "en".to_string()
}

/// Persist a validated language choice (guild-specific, or DM/global).
pub async fn set_user_lang(
    db: &Db,
    guild_id: Option<u64>,
    user_id: u64,
    lang: &str,
) -> sqlx::Result<()> {
    if !crate::translate::is_supported_lang(lang) {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO user_language_prefs (guild_id, user_id, lang) VALUES (?, ?, ?)
         ON CONFLICT(guild_id, user_id) DO UPDATE SET lang = excluded.lang, updated_at = datetime('now')",
    )
    .bind(resolve_lang_guild_id(guild_id))
    .bind(user_id.to_string())
    .bind(lang)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Default, FromRow)]
pub struct TranslatableMessage {
    pub message_id: Option<String>,
    pub guild_id: Option<String>,
    pub channel_id: Option<String>,
    pub kind: Option<String>,
    pub embed_json: Option<String>,
    pub content: Option<String>,
    pub owner_id: Option<String>,
}

pub struct NewTranslatable<'a> {
    pub guild_id: Option<u64>,
    pub channel_id: Option<u64>,
    pub kind: &'a str,
    /// Raw embed JSON (serde_json::Value of the embed), if any.
    pub embed_json: Option<String>,
    pub content: Option<String>,
    pub owner_id: Option<u64>,
}

pub async fn register(db: &Db, message_id: u64, input: NewTranslatable<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO translatable_messages (message_id, guild_id, channel_id, kind, embed_json, content, owner_id)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(message_id) DO UPDATE SET
            guild_id = excluded.guild_id, channel_id = excluded.channel_id,
            kind = excluded.kind, embed_json = excluded.embed_json,
            content = excluded.content, owner_id = excluded.owner_id",
    )
    .bind(message_id.to_string())
    .bind(input.guild_id.map(|id| id.to_string()))
    .bind(input.channel_id.map(|id| id.to_string()))
    .bind(input.kind)
    .bind(input.embed_json)
    .bind(input.content)
    .bind(input.owner_id.map(|id| id.to_string()))
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn get(db: &Db, message_id: u64) -> sqlx::Result<Option<TranslatableMessage>> {
    let row = sqlx::query_as::<_, TranslatableMessage>(
        "SELECT message_id, guild_id, channel_id, kind, embed_json, content, owner_id
         FROM translatable_messages WHERE message_id = ?",
    )
    .bind(message_id.to_string())
    .fetch_optional(&db.pool)
    .await?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dm_sentinel_resolution() {
        assert_eq!(resolve_lang_guild_id(None), DM_GUILD_ID);
        assert_eq!(resolve_lang_guild_id(Some(0)), DM_GUILD_ID);
        assert_eq!(resolve_lang_guild_id(Some(123)), "123");
    }

    #[tokio::test]
    async fn language_prefs_are_guild_scoped_with_dm_fallback() {
        let db = crate::db::Db::memory().await.unwrap();
        let user = 42u64;
        assert_eq!(get_user_lang(&db, None, user).await, "en");

        set_user_lang(&db, None, user, "de").await.unwrap();
        assert_eq!(get_user_lang(&db, None, user).await, "de");
        // A guild with no saved preference falls back to the DM/global one.
        assert_eq!(get_user_lang(&db, Some(100), user).await, "de");

        set_user_lang(&db, Some(100), user, "fr").await.unwrap();
        assert_eq!(get_user_lang(&db, Some(100), user).await, "fr");
        assert_eq!(get_user_lang(&db, Some(200), user).await, "de");

        // Updating replaces the stored value; invalid codes are ignored.
        set_user_lang(&db, Some(100), user, "ja").await.unwrap();
        assert_eq!(get_user_lang(&db, Some(100), user).await, "ja");
        set_user_lang(&db, Some(100), user, "xx").await.unwrap();
        assert_eq!(get_user_lang(&db, Some(100), user).await, "ja");
    }

    #[tokio::test]
    async fn translatable_rows_roundtrip() {
        let db = crate::db::Db::memory().await.unwrap();
        register(
            &db,
            99,
            NewTranslatable {
                guild_id: Some(7),
                channel_id: Some(8),
                kind: "test",
                embed_json: Some("{\"title\":\"T\"}".to_string()),
                content: Some("C".to_string()),
                owner_id: Some(42),
            },
        )
        .await
        .unwrap();
        let row = get(&db, 99).await.unwrap().unwrap();
        assert_eq!(row.embed_json.as_deref(), Some("{\"title\":\"T\"}"));
        assert_eq!(row.content.as_deref(), Some("C"));
        assert_eq!(row.kind.as_deref(), Some("test"));
    }
}

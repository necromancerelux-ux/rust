//! The message library: groups of reusable embed/plain messages.
//!
//! Uses the legacy `message_groups`, `group_admin_access`, `message_library`
//! and `message_library_embeds` tables verbatim.

use sqlx::FromRow;

use super::Db;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Bot owner only.
    Owner,
    /// Creator plus explicitly granted admins.
    Selected,
    /// Every admin of the guild.
    All,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Selected => "selected",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct MessageGroup {
    pub id: i64,
    pub name: String,
    pub guild_id: Option<String>,
    pub visibility: String,
    pub created_by: String,
    pub created_at: String,
}

impl MessageGroup {
    pub fn visibility(&self) -> Visibility {
        match self.visibility.as_str() {
            "selected" => Visibility::Selected,
            "all" => Visibility::All,
            _ => Visibility::Owner,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct LibraryMessage {
    pub id: i64,
    pub group_id: i64,
    pub name: String,
    pub is_embed: i64,
    pub plain_text: Option<String>,
    pub mention_type: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

impl LibraryMessage {
    pub fn is_embed(&self) -> bool {
        self.is_embed != 0
    }
}

#[derive(Debug, Clone, Default, FromRow)]
pub struct MessageEmbed {
    pub library_id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub color: Option<i64>,
    pub image_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub footer: Option<String>,
    pub author: Option<String>,
    pub images: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct TrackedMessage {
    pub id: i64,
    pub guild_id: String,
    pub channel_id: String,
    pub message_id: String,
    pub is_embed: i64,
    pub plain_text: Option<String>,
    pub embed_title: Option<String>,
    pub embed_description: Option<String>,
    pub embed_color: Option<i64>,
    pub embed_image_url: Option<String>,
    pub embed_thumbnail_url: Option<String>,
    pub embed_footer: Option<String>,
    pub embed_author: Option<String>,
    pub embed_images: Option<String>,
    pub library_id: Option<i64>,
    pub sent_by: String,
    pub sent_at: Option<String>,
}

// ----------------------------------------------------------------- groups

/// Groups the invoking admin can see. The creator and owner-only groups follow
/// the legacy rules: owner sees all, creator sees their own, granted admins
/// see selected groups, everyone sees 'all' groups.
pub async fn visible_groups(
    db: &Db,
    guild_id: u64,
    admin_id: u64,
    is_owner: bool,
) -> sqlx::Result<Vec<MessageGroup>> {
    if is_owner {
        return sqlx::query_as::<_, MessageGroup>(
            "SELECT * FROM message_groups WHERE guild_id IS NULL OR guild_id = ? ORDER BY name",
        )
        .bind(guild_id.to_string())
        .fetch_all(&db.pool)
        .await;
    }

    sqlx::query_as::<_, MessageGroup>(
        "SELECT DISTINCT g.* FROM message_groups g
         LEFT JOIN group_admin_access a ON a.group_id = g.id AND a.admin_id = ?
         WHERE g.guild_id = ?
           AND (g.created_by = ? OR g.visibility = 'all'
                OR (g.visibility = 'selected' AND a.admin_id IS NOT NULL))
         ORDER BY g.name",
    )
    .bind(admin_id.to_string())
    .bind(guild_id.to_string())
    .bind(admin_id.to_string())
    .fetch_all(&db.pool)
    .await
}

pub async fn list_all_groups(db: &Db) -> sqlx::Result<Vec<MessageGroup>> {
    sqlx::query_as::<_, MessageGroup>("SELECT * FROM message_groups ORDER BY name, id")
        .fetch_all(&db.pool)
        .await
}

pub async fn get_group(db: &Db, id: i64) -> sqlx::Result<Option<MessageGroup>> {
    sqlx::query_as::<_, MessageGroup>("SELECT * FROM message_groups WHERE id = ?")
        .bind(id)
        .fetch_optional(&db.pool)
        .await
}

pub async fn group_access_ids(db: &Db, group_id: i64) -> sqlx::Result<Vec<u64>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT admin_id FROM group_admin_access WHERE group_id = ? ORDER BY admin_id",
    )
    .bind(group_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id,)| id.parse().ok())
        .collect())
}

pub async fn set_group_visibility_global(
    db: &Db,
    id: i64,
    visibility: Visibility,
) -> sqlx::Result<bool> {
    let result = sqlx::query("UPDATE message_groups SET visibility = ? WHERE id = ?")
        .bind(visibility.as_str())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_group_access_ids(db: &Db, group_id: i64, admin_ids: &[u64]) -> sqlx::Result<()> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM group_admin_access WHERE group_id = ?")
        .bind(group_id)
        .execute(&mut *tx)
        .await?;
    for admin_id in admin_ids.iter().copied().take(25) {
        sqlx::query("INSERT INTO group_admin_access (group_id, admin_id) VALUES (?, ?)")
            .bind(group_id)
            .bind(admin_id.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn create_group(
    db: &Db,
    guild_id: u64,
    name: &str,
    visibility: Visibility,
    created_by: u64,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO message_groups (name, guild_id, visibility, created_by) VALUES (?, ?, ?, ?)",
    )
    .bind(name.trim())
    .bind(guild_id.to_string())
    .bind(visibility.as_str())
    .bind(created_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn delete_group(db: &Db, guild_id: u64, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM message_groups WHERE id = ? AND guild_id = ?")
        .bind(id)
        .bind(guild_id.to_string())
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn set_group_visibility(
    db: &Db,
    guild_id: u64,
    id: i64,
    visibility: Visibility,
) -> sqlx::Result<bool> {
    let result =
        sqlx::query("UPDATE message_groups SET visibility = ? WHERE id = ? AND guild_id = ?")
            .bind(visibility.as_str())
            .bind(id)
            .bind(guild_id.to_string())
            .execute(&db.pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn grant_admin(db: &Db, group_id: i64, admin_id: u64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO group_admin_access (group_id, admin_id) VALUES (?, ?)
         ON CONFLICT(group_id, admin_id) DO NOTHING",
    )
    .bind(group_id)
    .bind(admin_id.to_string())
    .execute(&db.pool)
    .await?;
    Ok(())
}

// --------------------------------------------------------------- messages

pub async fn list_messages(db: &Db, group_id: i64) -> sqlx::Result<Vec<LibraryMessage>> {
    sqlx::query_as::<_, LibraryMessage>(
        "SELECT * FROM message_library WHERE group_id = ? ORDER BY name",
    )
    .bind(group_id)
    .fetch_all(&db.pool)
    .await
}

#[derive(Debug, Clone, Default)]
pub struct MessageInput<'a> {
    pub name: &'a str,
    pub is_embed: bool,
    pub plain_text: Option<&'a str>,
    pub mention_type: Option<&'a str>,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub color: Option<i64>,
    pub footer: Option<&'a str>,
    pub author: Option<&'a str>,
    pub image_url: Option<&'a str>,
    pub thumbnail_url: Option<&'a str>,
}

pub async fn upsert_message(
    db: &Db,
    group_id: i64,
    input: &MessageInput<'_>,
    created_by: u64,
) -> anyhow::Result<i64> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM message_library WHERE group_id = ? AND name = ?")
            .bind(group_id)
            .bind(input.name.trim())
            .fetch_optional(&db.pool)
            .await?;

    let id = if let Some((id,)) = row {
        sqlx::query(
            "UPDATE message_library SET is_embed = ?, plain_text = ?, mention_type = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(input.is_embed as i64)
        .bind(input.plain_text.map(str::trim))
        .bind(input.mention_type.map(str::trim))
        .bind(id)
        .execute(&db.pool)
        .await?;
        id
    } else {
        let result = sqlx::query(
            "INSERT INTO message_library (group_id, name, is_embed, plain_text, mention_type, created_by)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(group_id)
        .bind(input.name.trim())
        .bind(input.is_embed as i64)
        .bind(input.plain_text.map(str::trim))
        .bind(input.mention_type.map(str::trim))
        .bind(created_by.to_string())
        .execute(&db.pool)
        .await?;
        result.last_insert_rowid()
    };

    // Upsert the embed payload even for plain messages (cleared to keep it tidy).
    sqlx::query(
        "INSERT INTO message_library_embeds
             (library_id, title, description, color, image_url, thumbnail_url, footer, author, images)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, NULL)
         ON CONFLICT(library_id) DO UPDATE SET
             title = excluded.title,
             description = excluded.description,
             color = excluded.color,
             image_url = excluded.image_url,
             thumbnail_url = excluded.thumbnail_url,
             footer = excluded.footer,
             author = excluded.author",
    )
    .bind(id)
    .bind(if input.is_embed { input.title.map(str::trim) } else { None })
    .bind(if input.is_embed { input.description.map(str::trim) } else { None })
    .bind(input.color)
    .bind(input.image_url.map(str::trim))
    .bind(input.thumbnail_url.map(str::trim))
    .bind(input.footer.map(str::trim))
    .bind(input.author.map(str::trim))
    .execute(&db.pool)
    .await?;

    Ok(id)
}

pub async fn get_message(
    db: &Db,
    group_id: i64,
    name: &str,
) -> sqlx::Result<Option<(LibraryMessage, MessageEmbed)>> {
    let Some(message) = sqlx::query_as::<_, LibraryMessage>(
        "SELECT * FROM message_library WHERE group_id = ? AND name = ?",
    )
    .bind(group_id)
    .bind(name.trim())
    .fetch_optional(&db.pool)
    .await?
    else {
        return Ok(None);
    };

    let embed = sqlx::query_as::<_, MessageEmbed>(
        "SELECT * FROM message_library_embeds WHERE library_id = ?",
    )
    .bind(message.id)
    .fetch_optional(&db.pool)
    .await?
    .unwrap_or(MessageEmbed {
        library_id: message.id,
        ..Default::default()
    });

    Ok(Some((message, embed)))
}

pub async fn get_library_message(
    db: &Db,
    group_id: i64,
    id: i64,
) -> sqlx::Result<Option<(LibraryMessage, MessageEmbed)>> {
    let Some(message) = sqlx::query_as::<_, LibraryMessage>(
        "SELECT * FROM message_library WHERE group_id = ? AND id = ?",
    )
    .bind(group_id)
    .bind(id)
    .fetch_optional(&db.pool)
    .await?
    else {
        return Ok(None);
    };
    let embed = sqlx::query_as::<_, MessageEmbed>(
        "SELECT * FROM message_library_embeds WHERE library_id = ?",
    )
    .bind(id)
    .fetch_optional(&db.pool)
    .await?
    .unwrap_or(MessageEmbed {
        library_id: id,
        ..Default::default()
    });
    Ok(Some((message, embed)))
}

pub async fn update_library_plain_text(
    db: &Db,
    group_id: i64,
    id: i64,
    text: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE message_library SET plain_text = ?, updated_at = datetime('now') WHERE group_id = ? AND id = ? AND is_embed = 0",
    )
    .bind(text)
    .bind(group_id)
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_library_embed_field(
    db: &Db,
    group_id: i64,
    id: i64,
    field: &str,
    value: Option<&str>,
) -> sqlx::Result<bool> {
    let column = match field {
        "title" => "title",
        "description" => "description",
        "color" => "color",
        "image_url" => "image_url",
        "thumbnail_url" => "thumbnail_url",
        "footer" => "footer",
        "author" => "author",
        "images" => "images",
        _ => return Ok(false),
    };
    let exists: Option<(i64,)> = sqlx::query_as(
        "SELECT e.library_id FROM message_library_embeds e JOIN message_library m ON m.id = e.library_id WHERE e.library_id = ? AND m.group_id = ? AND m.is_embed = 1",
    )
    .bind(id)
    .bind(group_id)
    .fetch_optional(&db.pool)
    .await?;
    if exists.is_none() {
        return Ok(false);
    }
    let sql = format!("UPDATE message_library_embeds SET {column} = ? WHERE library_id = ?");
    if field == "color" {
        let color =
            value.and_then(|value| i64::from_str_radix(value.trim_start_matches('#'), 16).ok());
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(color)
            .bind(id)
            .execute(&db.pool)
            .await?;
    } else {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(value)
            .bind(id)
            .execute(&db.pool)
            .await?;
    }
    sqlx::query("UPDATE message_library SET updated_at = datetime('now') WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(true)
}

pub async fn delete_library_message(db: &Db, group_id: i64, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM message_library WHERE group_id = ? AND id = ?")
        .bind(group_id)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn group_message_count(db: &Db, group_id: i64) -> sqlx::Result<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM message_library WHERE group_id = ?")
        .bind(group_id)
        .fetch_one(&db.pool)
        .await?;
    Ok(n)
}

// ------------------------------------------------------- sent-message tracking

pub async fn track_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
    library_id: Option<i64>,
    sent_by: u64,
    plain_text: &str,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO tracked_messages (guild_id, channel_id, message_id, is_embed, plain_text, library_id, sent_by) VALUES (?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(message_id.to_string())
    .bind(plain_text)
    .bind(library_id)
    .bind(sent_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn track_embed_message(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    message_id: u64,
    library_id: Option<i64>,
    sent_by: u64,
    embed: &MessageEmbed,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO tracked_messages (guild_id, channel_id, message_id, is_embed, embed_title, embed_description, embed_color, embed_image_url, embed_thumbnail_url, embed_footer, embed_author, embed_images, library_id, sent_by) VALUES (?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(message_id.to_string())
    .bind(&embed.title)
    .bind(&embed.description)
    .bind(embed.color)
    .bind(&embed.image_url)
    .bind(&embed.thumbnail_url)
    .bind(&embed.footer)
    .bind(&embed.author)
    .bind(&embed.images)
    .bind(library_id)
    .bind(sent_by.to_string())
    .execute(&db.pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn tracked_message_count(db: &Db, guild_id: u64, channel_id: u64) -> sqlx::Result<i64> {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM tracked_messages WHERE guild_id = ? AND channel_id = ?",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .fetch_one(&db.pool)
    .await?;
    Ok(count)
}

pub async fn list_tracked_messages(
    db: &Db,
    guild_id: u64,
    channel_id: u64,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<TrackedMessage>> {
    sqlx::query_as::<_, TrackedMessage>(
        "SELECT * FROM tracked_messages WHERE guild_id = ? AND channel_id = ? ORDER BY sent_at DESC, id DESC LIMIT ? OFFSET ?",
    )
    .bind(guild_id.to_string())
    .bind(channel_id.to_string())
    .bind(limit.clamp(1, 25))
    .bind(offset.max(0))
    .fetch_all(&db.pool)
    .await
}

pub async fn get_tracked_message(
    db: &Db,
    guild_id: u64,
    id: i64,
) -> sqlx::Result<Option<TrackedMessage>> {
    sqlx::query_as::<_, TrackedMessage>(
        "SELECT * FROM tracked_messages WHERE guild_id = ? AND id = ?",
    )
    .bind(guild_id.to_string())
    .bind(id)
    .fetch_optional(&db.pool)
    .await
}

pub async fn update_tracked_plain_text(
    db: &Db,
    guild_id: u64,
    id: i64,
    text: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE tracked_messages SET plain_text = ? WHERE guild_id = ? AND id = ? AND is_embed = 0",
    )
    .bind(text)
    .bind(guild_id.to_string())
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_tracked_embed_field(
    db: &Db,
    guild_id: u64,
    id: i64,
    field: &str,
    value: Option<&str>,
) -> sqlx::Result<bool> {
    let column = match field {
        "title" => "embed_title",
        "description" => "embed_description",
        "color" => "embed_color",
        "image_url" => "embed_image_url",
        "thumbnail_url" => "embed_thumbnail_url",
        "footer" => "embed_footer",
        "author" => "embed_author",
        "images" => "embed_images",
        _ => return Ok(false),
    };
    let sql = format!(
        "UPDATE tracked_messages SET {column} = ? WHERE guild_id = ? AND id = ? AND is_embed = 1"
    );
    let result = if field == "color" {
        let color =
            value.and_then(|value| i64::from_str_radix(value.trim_start_matches('#'), 16).ok());
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(color)
            .bind(guild_id.to_string())
            .bind(id)
            .execute(&db.pool)
            .await?
    } else {
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(value)
            .bind(guild_id.to_string())
            .bind(id)
            .execute(&db.pool)
            .await?
    };
    Ok(result.rows_affected() > 0)
}

pub async fn delete_tracked_message(db: &Db, guild_id: u64, id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM tracked_messages WHERE guild_id = ? AND id = ?")
        .bind(guild_id.to_string())
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

//! Google OAuth + Forms/Sheets/Drive HTTP layer for Ministry forms.
//!
//! Mirrors the legacy `googleAuth.js` + `googleSheets.js` HTTP calls. Tokens
//! live in `ministry_google_tokens` (scope `global`); the access token is
//! refreshed transparently when it expires.

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::Config;
use crate::db::Db;

const SCOPES: &str = "https://www.googleapis.com/auth/forms.body https://www.googleapis.com/auth/forms.responses.readonly https://www.googleapis.com/auth/spreadsheets https://www.googleapis.com/auth/drive.file https://www.googleapis.com/auth/drive";

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
const FORMS_URL: &str = "https://forms.googleapis.com/v1/forms";
const SHEETS_URL: &str = "https://sheets.googleapis.com/v4/spreadsheets";
const DRIVE_URL: &str = "https://www.googleapis.com/drive/v3/files";

#[derive(Debug, Clone, Default)]
pub struct GoogleStatus {
    pub configured: bool,
    pub connected: bool,
    pub email: Option<String>,
}

fn http() -> reqwest::Client {
    crate::net::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| crate::net::client())
}

fn extract_code(input: &str) -> String {
    let mut raw = input.trim().to_string();
    if let Some(start) = raw.find("code=") {
        let after = &raw[start + 5..];
        let end = after.find(['&', '#']).unwrap_or(after.len());
        raw = after[..end].to_string();
    }
    if let Ok(decoded) = urlencoding::decode(&raw) {
        raw = decoded.into_owned();
    }
    raw.trim().to_string()
}

pub fn auth_url(cfg: &Config) -> Result<String> {
    let client_id = cfg
        .google_client_id
        .as_deref()
        .ok_or_else(|| anyhow!("GOOGLE_CLIENT_ID not set"))?;
    let params = [
        ("client_id", client_id.to_string()),
        ("redirect_uri", cfg.google_redirect_uri.clone()),
        ("response_type", "code".to_string()),
        ("scope", SCOPES.to_string()),
        ("access_type", "offline".to_string()),
        ("prompt", "consent".to_string()),
        ("state", "ministry".to_string()),
    ];
    let qs = params
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    Ok(format!("{AUTH_URL}?{qs}"))
}

#[derive(Debug)]
struct TokenRow {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expiry_ts: Option<i64>,
    email: Option<String>,
}

type TokenRowTuple = (Option<String>, Option<String>, Option<i64>, Option<String>);

async fn token_row(db: &Db) -> Result<Option<TokenRow>> {
    let row: Option<TokenRowTuple> = sqlx::query_as(
        "SELECT access_token, refresh_token, expiry_ts, email FROM ministry_google_tokens WHERE scope_key = 'global'",
    )
    .fetch_optional(&db.pool)
    .await?;
    Ok(
        row.map(|(access_token, refresh_token, expiry_ts, email)| TokenRow {
            access_token: crate::secrets::open_opt(access_token),
            refresh_token: crate::secrets::open_opt(refresh_token),
            expiry_ts,
            email,
        }),
    )
}

async fn save_tokens(
    db: &Db,
    access_token: Option<&str>,
    refresh_token: Option<&str>,
    expires_in: Option<i64>,
    email: Option<&str>,
) -> Result<()> {
    let expiry = expires_in
        .map(|secs| chrono::Utc::now().timestamp_millis() + secs.saturating_sub(60) * 1000);
    sqlx::query(
        "INSERT INTO ministry_google_tokens (scope_key, access_token, refresh_token, expiry_ts, email, updated_at)
         VALUES ('global', ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(scope_key) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = COALESCE(excluded.refresh_token, ministry_google_tokens.refresh_token),
            expiry_ts = COALESCE(excluded.expiry_ts, ministry_google_tokens.expiry_ts),
            email = COALESCE(excluded.email, ministry_google_tokens.email),
            updated_at = datetime('now')",
    )
    .bind(crate::secrets::seal_opt(access_token))
    .bind(crate::secrets::seal_opt(refresh_token))
    .bind(expiry)
    .bind(email)
    .execute(&db.pool)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error_description: Option<String>,
    error: Option<String>,
}

async fn userinfo_email(access_token: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct UserInfo {
        email: Option<String>,
    }
    let resp = http()
        .get(USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .ok()?;
    resp.json::<UserInfo>().await.ok()?.email
}

pub async fn exchange_code(cfg: &Config, db: &Db, code: &str) -> Result<()> {
    let client_id = cfg
        .google_client_id
        .as_deref()
        .ok_or_else(|| anyhow!("GOOGLE_CLIENT_ID not set"))?;
    let client_secret = cfg
        .google_client_secret
        .as_deref()
        .ok_or_else(|| anyhow!("GOOGLE_CLIENT_SECRET not set"))?;

    let resp = http()
        .post(TOKEN_URL)
        .form(&[
            ("code", extract_code(code).as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", cfg.google_redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;
    let data: TokenResponse = resp.json().await?;
    if let Some(error) = &data.error {
        return Err(anyhow!(
            "{}",
            data.error_description.as_deref().unwrap_or(error)
        ));
    }
    let email = match &data.access_token {
        Some(token) => userinfo_email(token).await,
        None => None,
    };
    save_tokens(
        db,
        data.access_token.as_deref(),
        data.refresh_token.as_deref(),
        data.expires_in,
        email.as_deref(),
    )
    .await
}

pub async fn refresh_token(cfg: &Config, db: &Db) -> Result<()> {
    let row = token_row(db)
        .await?
        .ok_or_else(|| anyhow!("google_not_connected"))?;
    let refresh = row
        .refresh_token
        .ok_or_else(|| anyhow!("no_refresh_token"))?;
    let client_id = cfg
        .google_client_id
        .as_deref()
        .ok_or_else(|| anyhow!("GOOGLE_CLIENT_ID not set"))?;
    let client_secret = cfg
        .google_client_secret
        .as_deref()
        .ok_or_else(|| anyhow!("GOOGLE_CLIENT_SECRET not set"))?;

    let resp = http()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;
    let data: TokenResponse = resp.json().await?;
    if let Some(error) = &data.error {
        return Err(anyhow!(
            "{}",
            data.error_description.as_deref().unwrap_or(error)
        ));
    }
    let access = data.access_token.clone();
    // Google rarely rotates the refresh token — keep the old one when absent.
    save_tokens(
        db,
        access.as_deref(),
        None,
        data.expires_in,
        row.email.as_deref(),
    )
    .await
}

pub async fn get_access_token(cfg: &Config, db: &Db) -> Result<String> {
    let row = token_row(db).await?;
    let (access, expiry) = match &row {
        Some(row) => (row.access_token.clone(), row.expiry_ts.unwrap_or(0)),
        None => (None, 0),
    };
    if access.is_none()
        && row
            .as_ref()
            .and_then(|r| r.refresh_token.as_deref())
            .is_none()
    {
        return Err(anyhow!("google_not_connected"));
    }
    if access.is_none() || chrono::Utc::now().timestamp_millis() >= expiry {
        refresh_token(cfg, db).await?;
        let row = token_row(db).await?;
        return row
            .and_then(|row| row.access_token)
            .ok_or_else(|| anyhow!("refresh did not return an access token"));
    }
    Ok(access.unwrap_or_default())
}

pub async fn status(cfg: &Config, db: &Db) -> GoogleStatus {
    let configured = cfg.google_configured();
    let row = token_row(db).await.ok().flatten();
    let connected = row
        .as_ref()
        .map(|row| row.refresh_token.is_some() || row.access_token.is_some())
        .unwrap_or(false);
    GoogleStatus {
        configured,
        connected,
        email: row.and_then(|row| row.email),
    }
}

pub async fn disconnect(db: &Db) -> Result<()> {
    sqlx::query("DELETE FROM ministry_google_tokens WHERE scope_key = 'global'")
        .execute(&db.pool)
        .await?;
    Ok(())
}

async fn api(cfg: &Config, db: &Db, url: &str, method: &str, body: Option<Value>) -> Result<Value> {
    let token = get_access_token(cfg, db).await?;
    let request = match method {
        "GET" => http().get(url),
        "POST" => http().post(url),
        "PATCH" => http().patch(url),
        "DELETE" => http().delete(url),
        other => return Err(anyhow!("unsupported method `{other}`")),
    };
    let request = request
        .bearer_auth(&token)
        .header("Content-Type", "application/json");
    let request = match body {
        Some(body) => request.json(&body),
        None => request,
    };
    let resp = request.send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    let data: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if !status.is_success() {
        let message = data
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Google API {status}"));
        return Err(anyhow!(message));
    }
    Ok(data)
}

pub async fn sheets_api(
    cfg: &Config,
    db: &Db,
    path: &str,
    method: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = format!("{SHEETS_URL}{path}");
    api(cfg, db, &url, method, body).await
}

pub async fn forms_api(
    cfg: &Config,
    db: &Db,
    path: &str,
    method: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{FORMS_URL}{path}")
    };
    api(cfg, db, &url, method, body).await
}

pub async fn drive_api(
    cfg: &Config,
    db: &Db,
    path: &str,
    method: &str,
    body: Option<Value>,
) -> Result<Value> {
    let url = format!("{DRIVE_URL}{path}");
    api(cfg, db, &url, method, body).await
}

/// Percent-encode the Sheets `values` range path segment.
pub fn encode_range(range: &str) -> String {
    urlencoding::encode(range).into_owned()
}

pub async fn read_values(
    cfg: &Config,
    db: &Db,
    spreadsheet_id: &str,
    range: &str,
) -> Result<Vec<Vec<String>>> {
    let data = sheets_api(
        cfg,
        db,
        &format!(
            "/{spreadsheet_id}/values/{}?majorDimension=ROWS",
            encode_range(range)
        ),
        "GET",
        None,
    )
    .await?;
    Ok(data
        .get("values")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    row.as_array()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|c| c.as_str().unwrap_or("").to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default())
}

pub async fn append_values(
    cfg: &Config,
    db: &Db,
    spreadsheet_id: &str,
    range: &str,
    values: Vec<Vec<String>>,
) -> Result<()> {
    sheets_api(
        cfg,
        db,
        &format!(
            "/{spreadsheet_id}/values/{}:append?valueInputOption=RAW&insertDataOption=INSERT_ROWS",
            encode_range(range)
        ),
        "POST",
        Some(json!({ "values": values })),
    )
    .await?;
    Ok(())
}

pub async fn batch_update(
    cfg: &Config,
    db: &Db,
    spreadsheet_id: &str,
    ranges: Vec<(&str, Vec<Vec<String>>)>,
) -> Result<()> {
    let data: Vec<Value> = ranges
        .into_iter()
        .map(|(range, values)| json!({ "range": range, "values": values }))
        .collect();
    sheets_api(
        cfg,
        db,
        &format!("/{spreadsheet_id}/values:batchUpdate"),
        "POST",
        Some(json!({ "valueInputOption": "RAW", "data": data })),
    )
    .await?;
    Ok(())
}

/// Create the Applicant + TD/CD/RD workspace spreadsheet with header rows.
/// Returns `(spreadsheetId, spreadsheetUrl, applicantGid, tdGid, cdGid, rdGid)`.
pub async fn create_workspace_spreadsheet(
    cfg: &Config,
    db: &Db,
    title: &str,
) -> Result<(String, String, String, String, String, String)> {
    let body = json!({
        "properties": { "title": title },
        "sheets": [
            { "properties": { "title": "Applicant Sheet", "index": 0 } },
            { "properties": { "title": "TD Finalization", "index": 1 } },
            { "properties": { "title": "CD Finalization", "index": 2 } },
            { "properties": { "title": "RD Finalization", "index": 3 } },
        ],
    });
    let created = sheets_api(cfg, db, "", "POST", Some(body)).await?;
    let spreadsheet_id = created
        .get("spreadsheetId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("spreadsheet_create_failed"))?
        .to_string();
    let spreadsheet_url = created
        .get("spreadsheetUrl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let mut gids = std::collections::HashMap::new();
    if let Some(sheets) = created.get("sheets").and_then(|v| v.as_array()) {
        for sheet in sheets {
            if let (Some(title), Some(gid)) = (
                sheet.pointer("/properties/title").and_then(|v| v.as_str()),
                sheet
                    .pointer("/properties/sheetId")
                    .and_then(|v| v.as_i64()),
            ) {
                gids.insert(title.to_string(), gid.to_string());
            }
        }
    }

    batch_update(
        cfg,
        db,
        &spreadsheet_id,
        vec![
            (
                "'Applicant Sheet'!A1:L1",
                vec![vec![
                    "Player Name".into(),
                    "Player ID".into(),
                    "Preferred Time (UTC)".into(),
                    "FC".into(),
                    "rFC".into(),
                    "TS".into(),
                    "CS".into(),
                    "RS".into(),
                    "GS".into(),
                    "TD".into(),
                    "CD".into(),
                    "RD".into(),
                ]],
            ),
            (
                "'TD Finalization'!A1:D1",
                vec![vec![
                    "Player Name".into(),
                    "Player ID".into(),
                    "Slot (UTC)".into(),
                    "Added By".into(),
                ]],
            ),
            (
                "'CD Finalization'!A1:D1",
                vec![vec![
                    "Player Name".into(),
                    "Player ID".into(),
                    "Slot (UTC)".into(),
                    "Added By".into(),
                ]],
            ),
            (
                "'RD Finalization'!A1:D1",
                vec![vec![
                    "Player Name".into(),
                    "Player ID".into(),
                    "Slot (UTC)".into(),
                    "Added By".into(),
                ]],
            ),
        ],
    )
    .await?;

    Ok((
        spreadsheet_id.clone(),
        spreadsheet_url,
        gids.get("Applicant Sheet").cloned().unwrap_or_default(),
        gids.get("TD Finalization").cloned().unwrap_or_default(),
        gids.get("CD Finalization").cloned().unwrap_or_default(),
        gids.get("RD Finalization").cloned().unwrap_or_default(),
    ))
}

/// Share a Drive file with anyone-with-the-link (writer, fall back to reader).
pub async fn share_anyone_with_link(
    cfg: &Config,
    db: &Db,
    file_id: &str,
    role: &str,
) -> Result<()> {
    let desired = if role == "reader" { "reader" } else { "writer" };
    // Try to create the permission; if it already exists Google errors or we
    // can leave the existing one alone.
    let body = json!({
        "type": "anyone",
        "role": desired,
        "allowFileDiscovery": false,
    });
    match drive_api(cfg, db, &format!("/{file_id}/permissions?supportsAllDrives=true&sendNotificationEmail=false&fields=id,type,role"), "POST", Some(body)).await {
        Ok(_) => Ok(()),
        Err(err) => {
            let message = err.to_string();
            if desired == "writer" && (message.contains("403") || message.contains("forbidden") || message.contains("insufficient")) {
                // Fall back to reader — better than no share.
                let body = json!({
                    "type": "anyone",
                    "role": "reader",
                    "allowFileDiscovery": false,
                });
                return drive_api(cfg, db, &format!("/{file_id}/permissions?supportsAllDrives=true&sendNotificationEmail=false&fields=id,type,role"), "POST", Some(body))
                    .await
                    .map(|_| ());
            }
            Err(err)
        }
    }
}

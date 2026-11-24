//! Private translated ballot menu (`ballot:tr:*`), mirroring JS
//! `ballots/translatePanel.js`. The shared ballot message is never edited;
//! every interaction answers with a private (ephemeral) response.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::{
    ComponentInteraction, CreateActionRow, CreateButton, CreateEmbed, CreateSelectMenu,
    CreateSelectMenuKind, CreateSelectMenuOption, EditInteractionResponse,
};

use crate::state::Data;

const TTL: Duration = Duration::from_secs(15 * 60);

/// JS `translatePanel.js:TEMPLATE` — static UI strings translated as one batch.
#[derive(Clone)]
struct BallotStrings {
    header: String,
    privacy_line: String,
    lang_line: String,
    dropdown_hint: String,
    status_open: String,
    status_ends: String,
    status_closed: String,
    votes_public: String,
    votes_private: String,
    final_votes: String,
    role_only: String,
    everyone: String,
    voted: String,
    pick1: String,
    pick_any: String,
    pick_up_to: String,
    question_n: String,
    not_answered: String,
    all_answered: String,
    answer_all: String,
    your_answers: String,
    no_vote: String,
    vote_label: String,
    change_label: String,
    remove_label: String,
    refresh_label: String,
    back_label: String,
    next_label: String,
    submit_label: String,
    results_label: String,
    pick_one: String,
    pick_any_placeholder: String,
    pick_up_to_placeholder: String,
    notice_submitted: String,
    notice_removed: String,
    notice_no_vote: String,
    notice_closed: String,
    notice_role: String,
    notice_final: String,
    notice_change_off: String,
    notice_expired: String,
}

fn english_strings() -> BallotStrings {
    BallotStrings {
        header: "🌐 Ballot — translated".into(),
        privacy_line: "🔒 Private menu — only you can see this message.".into(),
        lang_line: "Language: {lang}".into(),
        dropdown_hint:
            "↑ Dropdown = translate this menu · votes made here count on the public ballot.".into(),
        status_open: "⏳ Open until an admin closes it".into(),
        status_ends: "⏳ Closes {when}".into(),
        status_closed: "🔒 Voting closed — final results".into(),
        votes_public: "👁️ Votes are public".into(),
        votes_private: "🕵️ Votes visible to admins only".into(),
        final_votes: "🔐 Votes are final".into(),
        role_only: "🎟️ Only {role} can vote".into(),
        everyone: "🎟️ Everyone".into(),
        voted: "👥 Voted: {n}".into(),
        pick1: "pick 1".into(),
        pick_any: "pick any".into(),
        pick_up_to: "pick up to {n}".into(),
        question_n: "Question {i} of {n}".into(),
        not_answered: "not answered".into(),
        all_answered: "All questions answered — press Submit.".into(),
        answer_all: "Answer every question, then press Submit.".into(),
        your_answers: "Your answers".into(),
        no_vote: "You haven't voted yet.".into(),
        vote_label: "Vote".into(),
        change_label: "Change my vote".into(),
        remove_label: "Remove my vote".into(),
        refresh_label: "Refresh".into(),
        back_label: "Back".into(),
        next_label: "Next".into(),
        submit_label: "Submit ballot".into(),
        results_label: "Results".into(),
        pick_one: "Pick one…".into(),
        pick_any_placeholder: "Pick any that apply…".into(),
        pick_up_to_placeholder: "Pick up to {n}…".into(),
        notice_submitted: "✅ Ballot submitted — thank you!".into(),
        notice_removed: "↩️ Your vote was removed.".into(),
        notice_no_vote: "You haven't voted in this ballot.".into(),
        notice_closed: "⚠️ This ballot is closed.".into(),
        notice_role: "⛔ Only members with {role} can vote in this ballot.".into(),
        notice_final: "🔒 You already voted — votes in this ballot are final.".into(),
        notice_change_off: "🔒 Votes in this ballot are final.".into(),
        notice_expired: "⚠️ That voting session expired — press Vote again.".into(),
    }
}

fn strings_vec(strings: &BallotStrings) -> Vec<String> {
    vec![
        strings.header.clone(),
        strings.privacy_line.clone(),
        strings.lang_line.clone(),
        strings.dropdown_hint.clone(),
        strings.status_open.clone(),
        strings.status_ends.clone(),
        strings.status_closed.clone(),
        strings.votes_public.clone(),
        strings.votes_private.clone(),
        strings.final_votes.clone(),
        strings.role_only.clone(),
        strings.everyone.clone(),
        strings.voted.clone(),
        strings.pick1.clone(),
        strings.pick_any.clone(),
        strings.pick_up_to.clone(),
        strings.question_n.clone(),
        strings.not_answered.clone(),
        strings.all_answered.clone(),
        strings.answer_all.clone(),
        strings.your_answers.clone(),
        strings.no_vote.clone(),
        strings.vote_label.clone(),
        strings.change_label.clone(),
        strings.remove_label.clone(),
        strings.refresh_label.clone(),
        strings.back_label.clone(),
        strings.next_label.clone(),
        strings.submit_label.clone(),
        strings.results_label.clone(),
        strings.pick_one.clone(),
        strings.pick_any_placeholder.clone(),
        strings.pick_up_to_placeholder.clone(),
        strings.notice_submitted.clone(),
        strings.notice_removed.clone(),
        strings.notice_no_vote.clone(),
        strings.notice_closed.clone(),
        strings.notice_role.clone(),
        strings.notice_final.clone(),
        strings.notice_change_off.clone(),
        strings.notice_expired.clone(),
    ]
}

fn from_vec(values: &[String]) -> BallotStrings {
    BallotStrings {
        header: values[0].clone(),
        privacy_line: values[1].clone(),
        lang_line: values[2].clone(),
        dropdown_hint: values[3].clone(),
        status_open: values[4].clone(),
        status_ends: values[5].clone(),
        status_closed: values[6].clone(),
        votes_public: values[7].clone(),
        votes_private: values[8].clone(),
        final_votes: values[9].clone(),
        role_only: values[10].clone(),
        everyone: values[11].clone(),
        voted: values[12].clone(),
        pick1: values[13].clone(),
        pick_any: values[14].clone(),
        pick_up_to: values[15].clone(),
        question_n: values[16].clone(),
        not_answered: values[17].clone(),
        all_answered: values[18].clone(),
        answer_all: values[19].clone(),
        your_answers: values[20].clone(),
        no_vote: values[21].clone(),
        vote_label: values[22].clone(),
        change_label: values[23].clone(),
        remove_label: values[24].clone(),
        refresh_label: values[25].clone(),
        back_label: values[26].clone(),
        next_label: values[27].clone(),
        submit_label: values[28].clone(),
        results_label: values[29].clone(),
        pick_one: values[30].clone(),
        pick_any_placeholder: values[31].clone(),
        pick_up_to_placeholder: values[32].clone(),
        notice_submitted: values[33].clone(),
        notice_removed: values[34].clone(),
        notice_no_vote: values[35].clone(),
        notice_closed: values[36].clone(),
        notice_role: values[37].clone(),
        notice_final: values[38].clone(),
        notice_change_off: values[39].clone(),
        notice_expired: values[40].clone(),
    }
}

static STATIC_CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();

/// JS `voteTranslate.staticStrings`: one batched API call per language, cached
/// only on success; on failure the original English template is returned.
async fn static_strings(data: &Data, lang: &str) -> (BallotStrings, bool) {
    let english = english_strings();
    if lang == "en" {
        return (english, true);
    }
    let cache = STATIC_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache.lock().ok().and_then(|guard| guard.get(lang).cloned()) {
        if cached.len() == 41 {
            return (from_vec(&cached), true);
        }
    }
    let source = strings_vec(&english);
    let translated = crate::translate::translate_batch(&data.gift_http(), &source, lang).await;
    if translated.len() == source.len()
        && translated
            .iter()
            .zip(source.iter())
            .any(|(value, original)| value != original)
    {
        if let Ok(mut guard) = cache.lock() {
            guard.insert(lang.to_string(), translated.clone());
        }
        (from_vec(&translated), true)
    } else {
        (english, false)
    }
}

/// Fills `{name}` placeholders, including translator-added spaces.
fn fmt(template: &str, vars: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (name, value) in vars {
        let value = value.clone();
        out = out
            .replace(&format!("{{{name}}}"), &value)
            .replace(&format!("{{ {name} }}"), &value);
    }
    out
}

const FAILED_NOTE: &str = "⚠️ Translation is unavailable right now — showing the original text.";

#[derive(Clone)]
struct VoteState {
    guild_id: u64,
    question: usize,
    answers: Vec<Vec<usize>>,
    created: Instant,
}

fn states() -> &'static Mutex<HashMap<(i64, u64), VoteState>> {
    static STATES: OnceLock<Mutex<HashMap<(i64, u64), VoteState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_state(ballot_id: i64, user_id: u64) -> Option<VoteState> {
    let mut guard = states().lock().ok()?;
    let key = (ballot_id, user_id);
    let expired = guard
        .get(&key)
        .is_some_and(|state| state.created.elapsed() > TTL);
    if expired {
        guard.remove(&key);
        return None;
    }
    guard.get(&key).cloned()
}

fn set_state(ballot_id: i64, user_id: u64, state: VoteState) {
    if let Ok(mut guard) = states().lock() {
        if guard.len() > 2000 {
            guard.retain(|_, value| value.created.elapsed() <= TTL);
        }
        guard.insert((ballot_id, user_id), state);
    }
}

fn clear_state(ballot_id: i64, user_id: u64) {
    if let Ok(mut guard) = states().lock() {
        guard.remove(&(ballot_id, user_id));
    }
}

async fn user_lang(data: &Data, guild_id: u64, user_id: u64) -> String {
    let lang = crate::db::translate::get_user_lang(&data.db, Some(guild_id), user_id).await;
    if crate::translate::is_supported_lang(&lang) {
        lang
    } else {
        "en".to_string()
    }
}

/// Translate the dynamic strings of a ballot in one batch. Returns the
/// translated copy and whether every string translated successfully.
async fn localize_ballot(
    data: &Data,
    ballot: &crate::db::ballots::Ballot,
    lang: &str,
) -> (crate::db::ballots::Ballot, bool) {
    if lang == "en" {
        return (ballot.clone(), true);
    }
    let mut strings: Vec<String> = vec![ballot.title.clone(), ballot.description.clone()];
    for question in &ballot.questions {
        strings.push(question.text.clone());
        strings.extend(question.options.iter().cloned());
    }
    let refs: Vec<String> = strings.clone();
    let translated = crate::translate::translate_batch(&data.gift_http(), &refs, lang).await;
    let ok = translated.len() == strings.len()
        && translated
            .iter()
            .zip(strings.iter())
            .any(|(value, original)| value != original);
    let mut index = 0;
    let mut pick = || -> String {
        let value = translated.get(index).cloned().unwrap_or_default();
        index += 1;
        if value.trim().is_empty() {
            strings.get(index - 1).cloned().unwrap_or_default()
        } else {
            value
        }
    };
    let mut copy = ballot.clone();
    copy.title = pick();
    copy.description = pick();
    for question in &mut copy.questions {
        question.text = pick();
        for option in &mut question.options {
            *option = pick();
        }
    }
    (copy, ok)
}

fn lang_row(ballot_id: i64, lang: &str) -> CreateActionRow {
    let options = crate::translate::LANGUAGES
        .iter()
        .map(|(label, emoji, code)| {
            CreateSelectMenuOption::new(format!("{label} {emoji}"), *code)
                .default_selection(*code == lang)
        })
        .collect::<Vec<_>>();
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            format!("ballot:trlang:{ballot_id}"),
            CreateSelectMenuKind::String { options },
        )
        .placeholder(format!("🌐 {}", crate::translate::lang_label(lang)))
        .min_values(1)
        .max_values(1),
    )
}

fn answerable(ballot: &crate::db::ballots::Ballot) -> bool {
    ballot.is_active()
}

fn answered(state: &VoteState) -> bool {
    state.answers.iter().all(|picks| !picks.is_empty())
}

fn notice_line(notice: &str) -> String {
    if notice.is_empty() {
        String::new()
    } else {
        format!("**{notice}**\n\n")
    }
}

/// Maps an internal notice string to its translated JS template equivalent.
fn translate_notice(t: &BallotStrings, notice: &str) -> String {
    match notice {
        "This ballot is closed or you cannot vote in it." => t.notice_closed.clone(),
        "Votes in this ballot are final." => t.notice_change_off.clone(),
        "Your vote was removed." => t.notice_removed.clone(),
        "That voting session expired — press Vote again." => t.notice_expired.clone(),
        "Ballot submitted — thank you!" => t.notice_submitted.clone(),
        "Answer every question, then press Submit ballot." => t.answer_all.clone(),
        other => other.to_string(),
    }
}

async fn render_results(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    ballot: &crate::db::ballots::Ballot,
    lang: &str,
    notice: &str,
    translated_ok: bool,
) -> anyhow::Result<()> {
    let (local, ok) = localize_ballot(data, ballot, lang).await;
    let (t, static_ok) = static_strings(data, lang).await;
    let translated_ok = translated_ok && ok && static_ok;
    let (counts, voters) = crate::ballots::tally_view(&data.db, &local).await?;
    let user_id = interaction.user.id.get();
    let mine = crate::db::ballots::answers(&data.db, &local, user_id)
        .await
        .ok();
    let mut lines = format!(
        "{}{}\n{}\n{}\n\n",
        notice_line(&translate_notice(&t, notice)),
        t.privacy_line,
        fmt(
            &t.lang_line,
            &[("lang", crate::translate::lang_label(lang))]
        ),
        t.dropdown_hint
    );
    if !translated_ok && lang != "en" {
        lines.push_str(&format!("{FAILED_NOTE}\n\n"));
    }
    let status_line = if ballot.status == "closed" || !ballot.is_active() {
        t.status_closed.clone()
    } else if let Some(ends) = local.ends_at {
        let when = crate::ui::theme::utc_with_relative(ends / 1000);
        fmt(&t.status_ends, &[("when", when)])
    } else {
        t.status_open.clone()
    };
    lines.push_str(&status_line);
    lines.push_str(&format!(
        "\n{} · {}\n\n{}",
        if local.show_votes {
            t.votes_public.clone()
        } else {
            t.votes_private.clone()
        },
        if local.allow_change {
            "Votes can be changed".to_string()
        } else {
            t.final_votes.clone()
        },
        fmt(&t.voted, &[("n", voters.to_string())])
    ));
    if let Some(answers) = &mine {
        lines.push_str(&format!("\n\n**{}**", t.your_answers));
        for (qi, picks) in answers.iter().enumerate() {
            let text = picks
                .iter()
                .filter_map(|index| {
                    local
                        .questions
                        .get(qi)
                        .and_then(|q| q.options.get(*index))
                        .cloned()
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push_str(&format!(
                "\n**Q{}** {}",
                qi + 1,
                if text.is_empty() {
                    "—".to_string()
                } else {
                    text
                }
            ));
        }
    } else if local.is_active() {
        lines.push_str(&format!("\n\n{}", t.no_vote));
    }
    let mut embed = serenity::CreateEmbed::new()
        .title(format!(
            "{} {}",
            if local.is_active() { "🗳️" } else { "🔒" },
            local.title
        ))
        .description(lines)
        .color(if local.is_active() {
            crate::ui::theme::colors::BRAND
        } else {
            crate::ui::theme::colors::VIOLET
        });
    for (qi, question) in local.questions.iter().enumerate() {
        let question_counts = counts.get(qi).cloned().unwrap_or_default();
        let winners = crate::ballots::leaders(&question_counts);
        let rows = question
            .options
            .iter()
            .enumerate()
            .map(|(oi, label)| {
                let count = question_counts.get(oi).copied().unwrap_or(0);
                let mark = if !local.is_active() && winners.contains(&oi) {
                    "🏆"
                } else {
                    "▫️"
                };
                let pct = if voters > 0 {
                    (count * 100 + voters / 2) / voters
                } else {
                    0
                };
                format!("{mark} **{pct}%** · {count} — {label}")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let pick = if question.max_picks <= 1 {
            t.pick1.clone()
        } else if question.max_picks >= question.options.len() as i64 {
            t.pick_any.clone()
        } else {
            fmt(&t.pick_up_to, &[("n", question.max_picks.to_string())])
        };
        embed = embed.field(
            format!("Q{} · {}  ({pick})", qi + 1, question.text),
            rows,
            false,
        );
    }

    let mut buttons: Vec<CreateButton> = Vec::new();
    if answerable(&local) {
        buttons.push(components_button(
            format!("ballot:trvote:{}", local.id),
            if mine.is_some() {
                format!("🔄 {}", t.change_label)
            } else {
                format!("🗳️ {}", t.vote_label)
            },
        ));
    }
    buttons.push(components_button(
        format!("ballot:trrefresh:{}", local.id),
        format!("🔄 {}", t.refresh_label),
    ));
    if answerable(&local) && local.allow_change && mine.is_some() {
        buttons.push(components_button(
            format!("ballot:trclear:{}", local.id),
            format!("↩️ {}", t.remove_label),
        ));
    }
    let rows = vec![lang_row(local.id, lang), CreateActionRow::Buttons(buttons)];
    edit_private(ctx, interaction, embed, rows).await
}

fn components_button(id: String, label: impl Into<String>) -> CreateButton {
    serenity::CreateButton::new(id).label(label)
}

async fn render_vote(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    ballot: &crate::db::ballots::Ballot,
    lang: &str,
    state: &VoteState,
    notice: &str,
    translated_ok: bool,
) -> anyhow::Result<()> {
    let (local, ok) = localize_ballot(data, ballot, lang).await;
    let (t, static_ok) = static_strings(data, lang).await;
    let translated_ok = translated_ok && ok && static_ok;
    let question = &local.questions[state.question];
    let mut lines = format!(
        "{}{}\n{}\n{}\n\n",
        notice_line(&translate_notice(&t, notice)),
        t.privacy_line,
        fmt(
            &t.lang_line,
            &[("lang", crate::translate::lang_label(lang))]
        ),
        t.dropdown_hint
    );
    if !translated_ok && lang != "en" {
        lines.push_str(&format!("{FAILED_NOTE}\n\n"));
    }
    lines.push_str(&format!(
        "**{}**\n### {}\n\n",
        fmt(
            &t.question_n,
            &[
                ("i", (state.question + 1).to_string()),
                ("n", local.questions.len().to_string()),
            ]
        ),
        question.text
    ));
    for (qi, item) in local.questions.iter().enumerate() {
        let picks = &state.answers[qi];
        let mark = if qi == state.question {
            "▶️"
        } else if !picks.is_empty() {
            "✅"
        } else {
            "⬜"
        };
        let text = picks
            .iter()
            .filter_map(|index| item.options.get(*index).cloned())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push_str(&format!(
            "{mark} **Q{}** {}\n",
            qi + 1,
            if text.is_empty() {
                format!("_{}_", t.not_answered)
            } else {
                text
            }
        ));
    }
    lines.push_str(if answered(state) {
        &t.all_answered
    } else {
        &t.answer_all
    });

    let embed = serenity::CreateEmbed::new()
        .title(format!("🗳️ {}", local.title))
        .description(lines)
        .color(crate::ui::theme::colors::BRAND);
    let chosen = state.answers[state.question].clone();
    let options = question
        .options
        .iter()
        .enumerate()
        .map(|(index, label)| {
            CreateSelectMenuOption::new(format!("{}. {}", index + 1, label), index.to_string())
                .default_selection(chosen.contains(&index))
        })
        .collect::<Vec<_>>();
    let select = CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            format!("ballot:trvsel:{}:{}", local.id, state.question),
            CreateSelectMenuKind::String { options },
        )
        .placeholder(if question.max_picks <= 1 {
            t.pick_one.clone()
        } else if question.max_picks >= question.options.len() as i64 {
            t.pick_any_placeholder.clone()
        } else {
            fmt(
                &t.pick_up_to_placeholder,
                &[("n", question.max_picks.to_string())],
            )
        })
        .min_values(0)
        .max_values(
            question
                .max_picks
                .min(question.options.len() as i64)
                .max(1)
                .clamp(1, 25) as u8,
        ),
    );
    let nav = CreateActionRow::Buttons(vec![
        serenity::CreateButton::new(format!("ballot:trnav:{}:prev", local.id))
            .label(format!("⬜ {}", t.back_label))
            .disabled(state.question == 0),
        serenity::CreateButton::new(format!("ballot:trnav:{}:next", local.id))
            .label(format!("{} ▶", t.next_label))
            .disabled(state.question + 1 >= local.questions.len()),
        serenity::CreateButton::new(format!("ballot:trsubmit:{}", local.id))
            .label(format!("✅ {}", t.submit_label))
            .disabled(!answered(state)),
        serenity::CreateButton::new(format!("ballot:trback:{}", local.id))
            .label(format!("📊 {}", t.results_label)),
    ]);
    edit_private(
        ctx,
        interaction,
        embed,
        vec![lang_row(local.id, lang), select, nav],
    )
    .await
}

async fn edit_private(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    embed: CreateEmbed,
    rows: Vec<CreateActionRow>,
) -> anyhow::Result<()> {
    interaction
        .create_response(
            &ctx.http,
            serenity::CreateInteractionResponse::Defer(
                serenity::CreateInteractionResponseMessage::new().ephemeral(true),
            ),
        )
        .await?;
    interaction
        .edit_response(
            &ctx.http,
            EditInteractionResponse::new().embed(embed).components(rows),
        )
        .await?;
    Ok(())
}

/// Handle a `ballot:tr*` sub-action. Returns true when handled.
pub async fn handle(
    ctx: &serenity::Context,
    data: &Data,
    interaction: &ComponentInteraction,
    action: &str,
) -> anyhow::Result<bool> {
    let (kind, remainder) = match action.split_once(':') {
        Some(parts) => parts,
        None => return Ok(false),
    };
    if !matches!(
        kind,
        "tr" | "trlang"
            | "trvote"
            | "trvsel"
            | "trnav"
            | "trsubmit"
            | "trback"
            | "trclear"
            | "trrefresh"
    ) {
        return Ok(false);
    }
    let mut parts = remainder.split(':');
    let id: i64 = match parts.next().and_then(|value| value.parse().ok()) {
        Some(id) => id,
        None => return Ok(false),
    };
    let arg = parts.next().unwrap_or("");
    let guild_id = interaction.guild_id.map(|id| id.get()).unwrap_or(0);
    let user_id = interaction.user.id.get();
    let Some(ballot) = crate::db::ballots::get(&data.db, id).await? else {
        return Ok(true);
    };
    if ballot.guild_id != guild_id.to_string() {
        return Ok(true);
    }
    let lang = user_lang(data, guild_id, user_id).await;

    if kind == "trlang" {
        let selected = match &interaction.data.kind {
            serenity::ComponentInteractionDataKind::StringSelect { values } => {
                values.first().cloned().unwrap_or_default()
            }
            _ => String::new(),
        };
        if crate::translate::is_supported_lang(&selected) {
            let _ =
                crate::db::translate::set_user_lang(&data.db, Some(guild_id), user_id, &selected)
                    .await;
        }
        let lang = user_lang(data, guild_id, user_id).await;
        let state = get_state(id, user_id);
        return match state {
            Some(state) if ballot.is_active() => {
                render_vote(ctx, data, interaction, &ballot, &lang, &state, "", true).await
            }
            _ => render_results(ctx, data, interaction, &ballot, &lang, "", true).await,
        }
        .map(|_| true);
    }
    if kind == "tr" || kind == "trback" {
        clear_state(id, user_id);
        return render_results(ctx, data, interaction, &ballot, &lang, "", true)
            .await
            .map(|_| true);
    }
    if kind == "trrefresh" {
        let state = get_state(id, user_id);
        return match state {
            Some(state) if ballot.is_active() => {
                render_vote(ctx, data, interaction, &ballot, &lang, &state, "", true).await
            }
            _ => render_results(ctx, data, interaction, &ballot, &lang, "", true).await,
        }
        .map(|_| true);
    }

    let role_ok = match ballot.expected_role_id() {
        Some(role_id) => interaction
            .member
            .as_ref()
            .is_some_and(|member| member.roles.iter().any(|id| id.get() == role_id)),
        None => true,
    };
    let can_vote = ballot.is_active()
        && role_ok
        && (ballot.allow_change
            || !crate::db::ballots::has_voted(&data.db, id, user_id)
                .await
                .unwrap_or(false));
    if !can_vote {
        clear_state(id, user_id);
        return render_results(
            ctx,
            data,
            interaction,
            &ballot,
            &lang,
            "This ballot is closed or you cannot vote in it.",
            true,
        )
        .await
        .map(|_| true);
    }

    if kind == "trvote" {
        let existing = crate::db::ballots::answers(&data.db, &ballot, user_id)
            .await
            .ok();
        let state = VoteState {
            guild_id,
            question: 0,
            answers: existing
                .unwrap_or_else(|| ballot.questions.iter().map(|_| Vec::new()).collect()),
            created: Instant::now(),
        };
        set_state(id, user_id, state.clone());
        return render_vote(ctx, data, interaction, &ballot, &lang, &state, "", true)
            .await
            .map(|_| true);
    }

    if kind == "trclear" {
        if !ballot.allow_change {
            return render_results(
                ctx,
                data,
                interaction,
                &ballot,
                &lang,
                "Votes in this ballot are final.",
                true,
            )
            .await
            .map(|_| true);
        }
        let _ = crate::db::ballots::withdraw(&data.db, id, user_id).await;
        clear_state(id, user_id);
        let _ = crate::ballots::rebuild_ballot_message(ctx, data, id).await;
        return render_results(
            ctx,
            data,
            interaction,
            &ballot,
            &lang,
            "Your vote was removed.",
            true,
        )
        .await
        .map(|_| true);
    }

    let Some(mut state) = get_state(id, user_id) else {
        return render_results(
            ctx,
            data,
            interaction,
            &ballot,
            &lang,
            "That voting session expired — press Vote again.",
            true,
        )
        .await
        .map(|_| true);
    };
    if state.guild_id != guild_id {
        return Ok(true);
    }

    match kind {
        "trvsel" => {
            let question_index: usize = arg.parse().unwrap_or(0);
            if question_index < state.answers.len() {
                let question = &ballot.questions[question_index];
                let mut picks = match &interaction.data.kind {
                    serenity::ComponentInteractionDataKind::StringSelect { values } => values
                        .iter()
                        .filter_map(|value| value.parse::<usize>().ok())
                        .filter(|index| *index < question.options.len())
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                picks.sort_unstable();
                picks.dedup();
                picks.truncate(question.max_picks.max(0) as usize);
                let single_auto_advance = question.max_picks <= 1
                    && !picks.is_empty()
                    && question_index + 1 < ballot.questions.len();
                state.answers[question_index] = picks;
                if single_auto_advance {
                    state.question = question_index + 1;
                }
                set_state(id, user_id, state.clone());
                return render_vote(ctx, data, interaction, &ballot, &lang, &state, "", true)
                    .await
                    .map(|_| true);
            }
        }
        "trnav" => {
            let last = ballot.questions.len().saturating_sub(1);
            state.question = if arg == "next" {
                (state.question + 1).min(last)
            } else {
                state.question.saturating_sub(1)
            };
            set_state(id, user_id, state.clone());
            return render_vote(ctx, data, interaction, &ballot, &lang, &state, "", true)
                .await
                .map(|_| true);
        }
        "trsubmit" => {
            if answered(&state) {
                crate::db::ballots::submit(&data.db, id, user_id, &state.answers).await?;
                clear_state(id, user_id);
                let _ = crate::ballots::rebuild_ballot_message(ctx, data, id).await;
                return render_results(
                    ctx,
                    data,
                    interaction,
                    &ballot,
                    &lang,
                    "Ballot submitted — thank you!",
                    true,
                )
                .await
                .map(|_| true);
            }
            return render_vote(
                ctx,
                data,
                interaction,
                &ballot,
                &lang,
                &state,
                "Answer every question, then press Submit ballot.",
                true,
            )
            .await
            .map(|_| true);
        }
        _ => {}
    }
    Ok(true)
}

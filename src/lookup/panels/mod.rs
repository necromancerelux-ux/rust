//! Components V2 Lookup panels (Player / Alliance / State).
//!
//! These replace the embed-based render path in `sessions::render` for a
//! pixel-matched UI against the JS bot (`src/lookup/components/*.js`).

pub mod alliance;
pub mod player;
pub mod state;

pub use alliance::edit_alliance_v2;
pub use player::edit_player_v2;
pub use state::edit_state_v2;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lookup::sessions::{Kind, LookupSession};
    use crate::ui::v2::V2Edit;
    use serde_json::json;
    use std::time::Instant;

    fn session(kind: Kind, raw: serde_json::Value) -> LookupSession {
        LookupSession {
            kind,
            raw,
            name: "Test".to_string(),
            state: Some(3105),
            fid: Some("501234".to_string()),
            alliance: Some("ABC".to_string()),
            tab: "overview".to_string(),
            notice: None,
            choices: Vec::new(),
            board: None,
            page: 0,
            map_token: None,
            created: Instant::now(),
        }
    }

    fn body_json(edit: V2Edit) -> String {
        serde_json::to_string(&edit.components).expect("components serialize")
    }

    #[test]
    fn player_overview_matches_js_ids_and_footer() {
        let s = session(
            Kind::Player,
            json!({ "name": "Chief", "uid": "123", "fid": "501234" }),
        );
        let body = body_json(edit_player_v2(
            &s,
            "overview",
            vec![("player.png".to_string(), vec![1, 2, 3])],
        ));
        assert!(body.contains("lookup:player:tab"), "{body}");
        assert!(body.contains("attachment://player.png"), "{body}");
        assert!(body.contains("lookup:player:go:state"), "{body}");
        assert!(body.contains("lookup:player:go:alliance"), "{body}");
        assert!(body.contains("lookup:player:go:trail"), "{body}");
        // JS only offers the map jump when coordinates are known.
        assert!(!body.contains("lookup:player:go:map"), "{body}");
        assert!(body.contains("profile as of"), "{body}");
    }

    #[test]
    fn player_standings_uses_js_board_selects_without_pager() {
        let s = session(
            Kind::Player,
            json!({
                "leaderboards": [
                    { "rank_type": "3", "score": 1_234_567, "rank": 5, "global_rank": 900, "global_total": 100_000 },
                    { "rank_type": "20", "score": 42, "rank": 1 }
                ]
            }),
        );
        let body = body_json(edit_player_v2(&s, "standings", Vec::new()));
        assert!(body.contains("lookup:player:board:state"), "{body}");
        assert!(body.contains("lookup:player:board:global"), "{body}");
        assert!(body.contains("#5 in S3105"), "{body}");
        // JS has no standings pager.
        assert!(!body.contains(":pg:p:"), "{body}");
        assert!(body.contains("boards as of"), "{body}");
    }

    #[test]
    fn player_heroes_renders_both_images_and_empty_state() {
        let s = session(Kind::Player, json!({ "name": "Chief" }));
        let body = body_json(edit_player_v2(
            &s,
            "heroes",
            vec![
                ("chief-gear.png".to_string(), vec![1]),
                ("arena.png".to_string(), vec![2]),
            ],
        ));
        assert!(body.contains("attachment://chief-gear.png"), "{body}");
        assert!(body.contains("attachment://arena.png"), "{body}");
        assert!(body.contains("loadout as of"), "{body}");

        let empty = {
            let e = edit_player_v2(&s, "heroes", Vec::new());
            body_json(e)
        };
        assert!(
            empty.contains("No chief gear or arena heroes stored for this chief yet."),
            "{empty}"
        );
    }

    #[test]
    fn player_map_subview_is_reachable() {
        let s = session(Kind::Player, json!({ "name": "Chief" }));
        let body = body_json(edit_player_v2(&s, "map", Vec::new()));
        assert!(body.contains("Location"), "{body}");
        assert!(body.contains("lookup:player:tab:overview"), "{body}");
        assert!(body.contains("position as of"), "{body}");
    }

    #[test]
    fn alliance_overview_order_and_footer() {
        let s = session(
            Kind::Alliance,
            json!({
                "alliance": { "abbr": "ABC", "name": "Alpha", "kid": 3105 },
                "roster": [
                    { "nick_name": "One", "fid": "1", "power": 900, "stove_lv": 30, "alliance_rank": 5 },
                    { "nick_name": "Two", "fid": "2", "power": 100, "stove_lv": 28, "alliance_rank": 1 }
                ]
            }),
        );
        let body = body_json(edit_alliance_v2(
            &s,
            "overview",
            vec![("alliance.png".to_string(), vec![1])],
        ));
        assert!(body.contains("lookup:alliance:tab"), "{body}");
        assert!(body.contains("lookup:alliance:go:map"), "{body}");
        assert!(body.contains("lookup:alliance:go:state"), "{body}");
        assert!(body.contains("lookup:alliance:player"), "{body}");
        assert!(body.contains("roster as of"), "{body}");
        // JS order: Open map first, then the state button.
        assert!(
            body.find("lookup:alliance:go:map").unwrap()
                < body.find("lookup:alliance:go:state").unwrap(),
            "{body}"
        );
    }

    #[test]
    fn alliance_members_sort_pager_and_footer() {
        let s = session(
            Kind::Alliance,
            json!({
                "alliance": { "abbr": "ABC", "name": "Alpha" },
                "roster": [
                    { "nick_name": "One", "fid": "1", "power": 900, "stove_lv": 30 },
                    { "nick_name": "Two", "fid": "2", "power": 100, "stove_lv": 28 }
                ]
            }),
        );
        let body = body_json(edit_alliance_v2(&s, "members", Vec::new()));
        assert!(body.contains("lookup:alliance:sort"), "{body}");
        assert!(body.contains("The Labyrinth score"), "{body}");
        assert!(body.contains("lookup:alliance:pg"), "{body}");
        assert!(body.contains("Page 1 / 1 · 2 members"), "{body}");
        assert!(body.contains("lookup:alliance:player"), "{body}");
        assert!(body.contains("**One**"), "{body}");
    }

    #[test]
    fn alliance_troop_level_and_furnace_pagers() {
        let s = session(
            Kind::Alliance,
            json!({
                "alliance": { "abbr": "ABC", "name": "Alpha" },
                "roster": [
                    { "nick_name": "One", "fid": "1", "power": 900, "stove_lv": 30,
                      "troopLevels": { "infantry": 12, "lancer": 11, "marksman": 11 } },
                    { "nick_name": "Two", "fid": "2", "power": 100, "stove_lv": 25 }
                ]
            }),
        );
        let tl = body_json(edit_alliance_v2(&s, "troopLevel", Vec::new()));
        assert!(tl.contains("lookup:alliance:tl:pg"), "{tl}");
        assert!(tl.contains("1/2 members with troop data"), "{tl}");
        assert!(tl.contains("lookup:alliance:tl:refresh"), "{tl}");
        assert!(tl.contains("lookup:alliance:tl:back"), "{tl}");
        assert!(
            tl.contains("troop levels as of") || tl.contains("with troop data"),
            "{tl}"
        );

        let furnace = body_json(edit_alliance_v2(&s, "furnace", Vec::new()));
        assert!(furnace.contains("lookup:alliance:pg"), "{furnace}");
        assert!(furnace.contains("Page 1 / 1"), "{furnace}");
        assert!(furnace.contains("**One** · F30"), "{furnace}");
    }

    #[test]
    fn alliance_notice_and_map_tabs() {
        let s = session(
            Kind::Alliance,
            json!({ "alliance": { "abbr": "ABC", "name": "Alpha", "notice": "Hello there" } }),
        );
        let notice = body_json(edit_alliance_v2(&s, "notice", Vec::new()));
        assert!(notice.contains("— Notice"), "{notice}");
        assert!(notice.contains("Hello there"), "{notice}");
        assert!(notice.contains("notice as of"), "{notice}");

        let blank = session(Kind::Alliance, json!({ "alliance": { "abbr": "ABC" } }));
        let empty = body_json(edit_alliance_v2(&blank, "notice", Vec::new()));
        assert!(
            empty.contains("This alliance has no notice or manifesto posted."),
            "{empty}"
        );

        let map = body_json(edit_alliance_v2(&s, "map", Vec::new()));
        assert!(map.contains("Hive map"), "{map}");
        assert!(map.contains("lookup:alliance:map:back"), "{map}");
        assert!(map.contains("lookup:alliance:map:refresh"), "{map}");
        assert!(map.contains("map as of"), "{map}");
    }

    #[test]
    fn state_overview_has_single_map_button() {
        let s = session(Kind::State, json!({ "kid": 3105, "name": "State 3105" }));
        let body = body_json(edit_state_v2(
            &s,
            "overview",
            vec![("state.png".to_string(), vec![1])],
        ));
        assert!(body.contains("lookup:state:tab"), "{body}");
        assert!(body.contains("attachment://state.png"), "{body}");
        assert!(body.contains("lookup:state:go:map"), "{body}");
        // battle/history/prep/president live on the SvS tab in JS.
        assert!(!body.contains("lookup:state:go:history"), "{body}");
        assert!(!body.contains("lookup:state:go:prep"), "{body}");
        assert!(body.contains("data as of"), "{body}");
    }

    #[test]
    fn state_svs_offers_conditional_subviews() {
        let s = session(
            Kind::State,
            json!({
                "kid": 3105,
                "matchup": { "season": 12, "opponent": { "kid": 3106 } },
                "kvk": { "history": [ { "opponent_kid": 3107, "prep": "won", "prep_score": 120000 } ] },
                "president_name": "Big Chief"
            }),
        );
        let body = body_json(edit_state_v2(&s, "svs", Vec::new()));
        assert!(
            body.contains("State 3105 - State vs State · Season 12"),
            "{body}"
        );
        assert!(body.contains("lookup:state:go:history"), "{body}");
        assert!(body.contains("lookup:state:go:prep"), "{body}");
        assert!(body.contains("lookup:state:go:president"), "{body}");
        assert!(body.contains("President - Big Chief"), "{body}");
        assert!(body.contains("data as of"), "{body}");
    }

    #[test]
    fn state_subviews_are_not_collapsed_to_overview() {
        let s = session(Kind::State, json!({ "kid": 3105 }));
        let prep = body_json(edit_state_v2(&s, "prep", Vec::new()));
        assert!(prep.contains("Prep points"), "{prep}");
        assert!(prep.contains("lookup:state:svs:back"), "{prep}");
        assert!(prep.contains("prep scores as of"), "{prep}");

        let battle = body_json(edit_state_v2(&s, "battle", Vec::new()));
        assert!(battle.contains("Sunfire Castle battle"), "{battle}");
        assert!(battle.contains("retrieved just now"), "{battle}");

        let note = body_json(edit_state_v2(&s, "note", Vec::new()));
        assert!(note.contains("lookup:state:overview:back"), "{note}");

        let history = body_json(edit_state_v2(&s, "history", Vec::new()));
        assert!(history.contains("Matchup history"), "{history}");
    }

    #[test]
    fn state_boards_select_pager_and_entries() {
        let s = session(
            Kind::State,
            json!({
                "kid": 3105,
                "boardEntries": [
                    { "rank": 1, "uid": "1001", "nick_name": "Top", "alliance_abbr": "ABC", "score": 9999, "furnace": 30 },
                    { "rank": 2, "fid": "1002", "nick_name": "Second", "score": 5000, "troopLevels": { "infantry": 12 } }
                ]
            }),
        );
        let body = body_json(edit_state_v2(&s, "boards", Vec::new()));
        assert!(
            body.contains("State 3105 — Personal Power · 2 ranked"),
            "{body}"
        );
        assert!(body.contains("lookup:state:board"), "{body}");
        assert!(body.contains("Cave of Monsters"), "{body}");
        assert!(body.contains("Core · top 100"), "{body}");
        assert!(body.contains("lookup:state:pg"), "{body}");
        assert!(body.contains("lookup:state:player"), "{body}");
        assert!(body.contains(" [I12]"), "{body}");
        assert!(body.contains("data as of"), "{body}");

        // Alliance boards switch to the alliance picker instead.
        let mut alliance_session = s.clone();
        alliance_session.raw["boardEntries"] = json!([
            { "rank": 1, "abbr": "AAA", "name": "Alpha", "aid": 7, "power": 1000, "members": 50 }
        ]);
        alliance_session.board = Some("1".to_string());
        let ab = {
            let e = edit_state_v2(&alliance_session, "boards", Vec::new());
            body_json(e)
        };
        assert!(ab.contains("lookup:state:alliance"), "{ab}");
        assert!(!ab.contains("lookup:state:player"), "{ab}");
    }

    #[test]
    fn state_troop_level_and_alliances_pagers() {
        let s = session(
            Kind::State,
            json!({
                "kid": 3105,
                "players": [
                    { "nick_name": "A", "score": 100, "troopLevels": { "infantry": 12, "lancer": 10, "marksman": 9 } },
                    { "nick_name": "B", "score": 50 }
                ],
                "alliances": [
                    { "abbr": "AAA", "name": "Alpha", "power": 1000, "members": 50, "kills": 2000 }
                ]
            }),
        );
        let tl = body_json(edit_state_v2(&s, "troopLevel", Vec::new()));
        assert!(tl.contains("lookup:state:tl:pg"), "{tl}");
        assert!(tl.contains("lookup:state:tl:refresh"), "{tl}");
        assert!(tl.contains("lookup:state:tl:back"), "{tl}");
        assert!(tl.contains("1/2 players with troop data"), "{tl}");
        assert!(tl.contains("troop levels as of"), "{tl}");

        let alliances = body_json(edit_state_v2(&s, "alliances", Vec::new()));
        assert!(alliances.contains("lookup:state:pg"), "{alliances}");
        assert!(alliances.contains("50 members"), "{alliances}");
        assert!(alliances.contains("2.0K kills"), "{alliances}");
        assert!(alliances.contains("lookup:state:alliance"), "{alliances}");
    }

    #[test]
    fn state_map_buttons_follow_js_order() {
        let s = session(Kind::State, json!({ "kid": 3105 }));
        let body = body_json(edit_state_v2(&s, "map", Vec::new()));
        let refresh = body.find("lookup:state:map:refresh").expect("refresh");
        let back = body.find("lookup:state:map:back").expect("back");
        assert!(refresh < back, "{body}");
        assert!(body.contains("map as of"), "{body}");
    }
}

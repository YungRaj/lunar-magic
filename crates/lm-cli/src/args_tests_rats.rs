use super::*;

#[test]
fn parses_rats_manifest_plan_and_reclamation_commands() {
    assert_eq!(
        parse_from(&[
            "rats-observe".into(),
            "installed.smc".into(),
            "rats.obs".into(),
        ])
        .unwrap(),
        Command::RatsObserve {
            rom: "installed.smc".into(),
            output: "rats.obs".into(),
        }
    );
    assert_eq!(
        parse_from(&["rats-manifest".into(), "owned.lmrats".into()]).unwrap(),
        Command::RatsManifest {
            input: "owned.lmrats".into(),
            normalized_output: None,
            observation: None,
        }
    );
    assert_eq!(
        parse_from(&[
            "rats-manifest".into(),
            "owned.lmrats".into(),
            "normalized.lmrats".into(),
            "owned.obs".into(),
        ])
        .unwrap(),
        Command::RatsManifest {
            input: "owned.lmrats".into(),
            normalized_output: Some("normalized.lmrats".into()),
            observation: Some("owned.obs".into()),
        }
    );
    assert_eq!(
        parse_from(&[
            "rats-plan".into(),
            "game.smc".into(),
            "owned.lmrats".into(),
            "0xff".into(),
        ])
        .unwrap(),
        Command::RatsPlan {
            rom: "game.smc".into(),
            manifest: "owned.lmrats".into(),
            fill: 0xff,
        }
    );
    assert_eq!(
        parse_from(&[
            "rats-reclaim".into(),
            "game.smc".into(),
            "clean.smc".into(),
            "owned.lmrats".into(),
            "0".into(),
        ])
        .unwrap(),
        Command::RatsReclaim {
            input: "game.smc".into(),
            output: "clean.smc".into(),
            manifest: "owned.lmrats".into(),
            fill: 0,
        }
    );
    assert!(
        parse_from(&[
            "rats-plan".into(),
            "game.smc".into(),
            "owned.lmrats".into(),
            "0x100".into(),
        ])
        .is_err()
    );
}

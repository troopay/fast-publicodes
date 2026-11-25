use std::sync::Arc;
use std::thread;

use fast_publicodes::serde_json::json;
use fast_publicodes::{Engine, Options, SituationOptions, Value};

#[test]
fn snapshots_evaluate_on_other_threads() {
    let rules = json!({
        "prix": 10,
        "quantité": { "par défaut": 2 },
        "total": "prix * quantité",
    });
    let today = Arc::new(|| Ok("25/11/2026".to_owned()));
    let engine = Engine::new(&rules, Options::default(), today, None).unwrap();
    let threads: Vec<_> = (1..=4)
        .map(|quantity| {
            let snapshot = engine.snapshot();
            thread::spawn(move || {
                let mut copy = Engine::from(snapshot);
                copy.set_situation(
                    &json!({ "quantité": quantity }),
                    SituationOptions::default(),
                )
                .unwrap();
                copy.evaluate(&json!("total")).unwrap().value
            })
        })
        .collect();
    for (quantity, thread) in (1..=4).zip(threads) {
        assert_eq!(
            thread.join().unwrap(),
            Value::Number(10. * f64::from(quantity))
        );
    }
}

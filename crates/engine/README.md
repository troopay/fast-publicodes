# fast-publicodes

The engine of [fast-publicodes](https://github.com/troopay/fast-publicodes): [publicodes](https://github.com/publicodes/publicodes) 1.x rewritten in Rust, with the same behaviour. Rules and situations are given as JSON, as `publicodes` reads them.

```rust
use std::rc::Rc;

use fast_publicodes::serde_json::json;
use fast_publicodes::{Engine, Options, SituationOptions, Value};

let rules = json!({
    "prix": 10,
    "quantité": { "par défaut": 2 },
    "total": "prix * quantité",
});
let today = Rc::new(|| Ok("25/11/2026".to_owned()));
let mut engine = Engine::new(&rules, Options::default(), today, None).unwrap();
engine
    .set_situation(&json!({ "quantité": 3 }), SituationOptions::default())
    .unwrap();
assert_eq!(engine.evaluate(&json!("total")).unwrap().value, Value::Number(30.));
```

An `Engine` stays on the thread that created it.

For full speed, build with link-time optimization in your `Cargo.toml`:

```toml
[profile.release]
lto = true
codegen-units = 1
```

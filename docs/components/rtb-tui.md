---
title: rtb-tui
description: Reusable TUI building blocks — Wizard, render helpers, TTY-aware Spinner.
date: 2026-05-06
tags: [components, tui, wizard, render, spinner]
authors: [Matt Cockayne <matt@phpboyscout.com>]
---

# `rtb-tui` v0.1

Three small building blocks every RTB-built CLI tool needs and would
otherwise have to roll itself.

## Public API

```rust
use rtb_tui::{Wizard, WizardStep, StepOutcome, render_table, render_json, Spinner};
```

| Item | Kind | Since |
|---|---|---|
| `Wizard<S>` | struct | 0.1.0 |
| `WizardBuilder<S>` | struct | 0.1.0 |
| `WizardStep<S>` | trait (async) | 0.1.0 |
| `StepOutcome` | enum (`Next`, `Back`) | 0.1.0 |
| `WizardError` | enum (`Cancelled`, `Interrupted`, `Step`) | 0.1.0 |
| `Spinner` | struct | 0.1.0 |
| `render_table<R: Tabled>(rows) -> String` | fn | 0.1.0 |
| `render_json<R: Serialize>(rows) -> Result<String, RenderError>` | fn | 0.1.0 |
| `RenderError` | enum (`Json`) | 0.1.0 |
| `InquireError` | re-export | 0.1.0 |

## `Wizard`

Multi-step interactive form backed by [`inquire`](https://crates.io/crates/inquire).

```rust
use rtb_tui::{Wizard, WizardStep, StepOutcome, InquireError};
use async_trait::async_trait;

struct Greet;

#[async_trait]
impl WizardStep<Profile> for Greet {
    fn name(&self) -> &'static str { "greet" }
    async fn prompt(&self, state: &mut Profile) -> Result<StepOutcome, InquireError> {
        state.greeting = Some(inquire::Text::new("Hello, what should I call you?").prompt()?);
        Ok(StepOutcome::Next)
    }
}

# async fn main() -> Result<(), rtb_tui::WizardError> {
let profile = Wizard::<Profile>::builder()
    .initial(Profile::default())
    .step(Greet)
    .build()
    .run()
    .await?;
# Ok(()) }
```

### Navigation rules

- A step that returns `StepOutcome::Next` advances; if it was the last step, `run` finishes.
- A step that returns `StepOutcome::Back` re-runs the previous step. If the wizard is on step 0, `run` returns `WizardError::Cancelled`.
- A step that returns `Err(InquireError::OperationCanceled)` (Esc) is treated identically to `StepOutcome::Back` — the driver maps it for you, so steps just `?`-propagate.
- `Err(InquireError::OperationInterrupted)` (Ctrl+C) short-circuits to `WizardError::Interrupted` regardless of position.
- Any other `InquireError` is wrapped in `WizardError::Step { step, message }` with the step's name attached for diagnosis.

### State threading

`Wizard<S>` owns its state. Each step receives `&mut S`, so step *N+1* sees mutations made by step *N*. When the user backs into a previous step, the step re-runs against the **current** state — implementations should be idempotent (using current state to default-fill `inquire` prompts is the canonical pattern).

## Render helpers

```rust
use rtb_tui::{render_table, render_json};
use serde::Serialize;
use tabled::Tabled;

#[derive(Tabled, Serialize)]
struct Row { name: &'static str, count: u32 }

let rows = vec![Row { name: "alpha", count: 1 }];

print!("{}", render_table(&rows));               // psql-style text table
print!("{}", render_json(&rows).unwrap());       // pretty-printed JSON array
```

Both helpers add a trailing newline so callers can `print!` directly without their own `println!`.

`render_table` is infallible (`tabled` cannot fail over a `Tabled`-deriving type). `render_json` returns `RenderError::Json(_)` when a row's `Serialize` impl fails — always programmer mistake (non-`Serialize`-clean shape), never user input.

## `Spinner`

```rust
use rtb_tui::Spinner;

let mut s = Spinner::new("downloading…");
// … work …
s.set_message("verifying signature…");
// … work …
s.finish();   // explicit; the Drop impl also calls finish()
```

When stderr isn't a TTY (CI logs, MCP-stdio transports), every method on `Spinner` is a no-op — no escape sequences leak into captured output. The spinner is single-threaded by design: there is no internal `tokio::task::spawn` that animates frames. Tick the spinner manually via `set_message` between awaits.

## Crate layout

```
crates/rtb-tui/
├── src/
│   ├── lib.rs        # public re-exports
│   ├── error.rs      # WizardError, RenderError
│   ├── wizard.rs     # Wizard, WizardBuilder, WizardStep, StepOutcome
│   ├── render.rs     # render_table, render_json
│   └── spinner.rs    # Spinner
└── tests/
    ├── wizard_back_navigation.rs
    ├── wizard_cancellation.rs
    ├── wizard_state_threading.rs
    ├── render_table_dual.rs
    └── spinner_no_tty.rs
```

## Spec

Authoritative contract:
[`docs/development/specs/2026-05-06-rtb-tui-v0.1.md`](../development/specs/2026-05-06-rtb-tui-v0.1.md).

//! Standard-library-only Rust reading exercises.
//!
//! Compile with:
//! `rustc --edition 2024 --test katas.rs -o /tmp/grok-rust-katas`

#![allow(dead_code)]

#[cfg(test)]
mod tests {
    use std::fmt;
    use std::num::ParseIntError;
    use std::sync::{Arc, Mutex, mpsc};
    use std::thread;

    // A newtype prevents a session ID from being confused with any String.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SessionId(String);

    impl SessionId {
        fn new(raw: impl Into<String>) -> Self {
            Self(raw.into())
        }

        fn as_str(&self) -> &str {
            &self.0
        }
    }

    #[test]
    fn kata_01_owned_and_borrowed_strings() {
        let id = SessionId::new("session-7");
        let shared_view = id.as_str();

        // Predict why both names remain usable and whether this allocates.
        assert_eq!(shared_view, "session-7");
        assert_eq!(id.as_str(), "session-7");

        // This clone creates another owned String; it is not just a borrow.
        let copied = id.clone();
        assert_eq!(copied, id);
    }

    #[derive(Debug, PartialEq, Eq)]
    enum TurnEvent {
        Started { prompt_id: String },
        Finished { prompt_id: String, tokens: u64 },
        Cancelled,
    }

    fn describe(event: &TurnEvent) -> String {
        match event {
            TurnEvent::Started { prompt_id } => format!("start:{prompt_id}"),
            TurnEvent::Finished { prompt_id, tokens } => {
                format!("finish:{prompt_id}:{tokens}")
            }
            TurnEvent::Cancelled => "cancelled".to_owned(),
        }
    }

    #[test]
    fn kata_02_match_a_borrow_without_moving_fields() {
        let event = TurnEvent::Finished {
            prompt_id: "p1".to_owned(),
            tokens: 42,
        };

        assert_eq!(describe(&event), "finish:p1:42");
        // `describe` matched `&TurnEvent`, so `event` still owns prompt_id.
        assert!(matches!(event, TurnEvent::Finished { tokens: 42, .. }));
    }

    fn completed_task_id(event: &TurnEvent) -> Option<&str> {
        let TurnEvent::Finished { prompt_id, .. } = event else {
            return None;
        };
        Some(prompt_id)
    }

    #[test]
    fn kata_03_let_else_narrows_one_shape() {
        let running = TurnEvent::Started {
            prompt_id: "p2".to_owned(),
        };
        let done = TurnEvent::Finished {
            prompt_id: "p3".to_owned(),
            tokens: 8,
        };

        assert_eq!(completed_task_id(&running), None);
        assert_eq!(completed_task_id(&done), Some("p3"));
    }

    fn parse_optional_limit(raw: Option<&str>) -> Result<Option<u32>, ParseIntError> {
        // map: Option<&str> -> Option<Result<u32, ParseIntError>>
        // transpose: Option<Result<T, E>> -> Result<Option<T>, E>
        raw.map(str::parse::<u32>).transpose()
    }

    #[test]
    fn kata_04_option_result_transpose() {
        assert_eq!(parse_optional_limit(None).unwrap(), None);
        assert_eq!(parse_optional_limit(Some("12")).unwrap(), Some(12));
        assert!(parse_optional_limit(Some("many")).is_err());
    }

    #[test]
    fn kata_05_iter_iter_mut_and_into_iter() {
        let mut names = vec!["read".to_owned(), "write".to_owned()];

        let lengths: Vec<usize> = names.iter().map(String::len).collect();
        assert_eq!(lengths, vec![4, 5]);
        assert_eq!(names.len(), 2); // iter() only borrowed the Vec.

        for name in names.iter_mut() {
            name.make_ascii_uppercase();
        }
        assert_eq!(names, vec!["READ", "WRITE"]);

        let owned: Vec<String> = names.into_iter().collect();
        assert_eq!(owned, vec!["READ", "WRITE"]);
        // `names` cannot be used here: into_iter() consumed it.
    }

    fn apply_twice(mut operation: impl FnMut(i32) -> i32, value: i32) -> i32 {
        let first = operation(value);
        operation(first)
    }

    #[test]
    fn kata_06_closure_capture_and_fnmut() {
        let mut calls = 0;
        let answer = apply_twice(
            |value| {
                calls += 1;
                value + calls
            },
            10,
        );

        assert_eq!(answer, 13); // (10 + 1) + 2
        assert_eq!(calls, 2);
    }

    trait Render {
        fn render(&self) -> String;

        fn is_empty(&self) -> bool {
            self.render().is_empty()
        }
    }

    impl Render for SessionId {
        fn render(&self) -> String {
            format!("session:{}", self.as_str())
        }
    }

    fn render_static(value: &impl Render) -> String {
        value.render()
    }

    fn render_dynamic(value: &dyn Render) -> String {
        value.render()
    }

    #[test]
    fn kata_07_static_and_dynamic_dispatch() {
        let id = SessionId::new("abc");
        assert_eq!(render_static(&id), "session:abc");
        assert_eq!(render_dynamic(&id), "session:abc");
        assert!(!id.is_empty());
    }

    trait Decode {
        type Output;
        type Error;

        fn decode(&self) -> Result<Self::Output, Self::Error>;
    }

    struct TokenCount<'a>(&'a str);

    impl Decode for TokenCount<'_> {
        type Output = u64;
        type Error = ParseIntError;

        fn decode(&self) -> Result<Self::Output, Self::Error> {
            self.0.parse()
        }
    }

    #[test]
    fn kata_08_associated_types_belong_to_the_impl() {
        let encoded = TokenCount("128");
        let decoded: u64 = encoded.decode().unwrap();
        assert_eq!(decoded, 128);
    }

    #[derive(Debug)]
    enum LimitError {
        InvalidNumber(ParseIntError),
        Zero,
    }

    impl From<ParseIntError> for LimitError {
        fn from(error: ParseIntError) -> Self {
            Self::InvalidNumber(error)
        }
    }

    fn positive_limit(raw: &str) -> Result<u32, LimitError> {
        let value = raw.parse::<u32>()?;
        if value == 0 {
            return Err(LimitError::Zero);
        }
        Ok(value)
    }

    #[test]
    fn kata_09_question_mark_uses_from() {
        assert_eq!(positive_limit("5").unwrap(), 5);
        assert!(matches!(positive_limit("0"), Err(LimitError::Zero)));
        assert!(matches!(
            positive_limit("none"),
            Err(LimitError::InvalidNumber(_))
        ));
    }

    trait Named {
        fn name(&self) -> &str;
    }

    impl Named for SessionId {
        fn name(&self) -> &str {
            self.as_str()
        }
    }

    // Blanket implementation: every Box<T> where T: Named is also Named.
    impl<T: Named + ?Sized> Named for Box<T> {
        fn name(&self) -> &str {
            (**self).name()
        }
    }

    #[test]
    fn kata_10_blanket_impl_and_unsized_trait_object() {
        let value: Box<dyn Named> = Box::new(SessionId::new("boxed"));
        assert_eq!(value.name(), "boxed");
    }

    fn choose_nonempty<'a>(primary: &'a str, fallback: &'a str) -> &'a str {
        if primary.is_empty() {
            fallback
        } else {
            primary
        }
    }

    #[test]
    fn kata_11_lifetime_connects_inputs_to_output() {
        let primary = String::new();
        let fallback = String::from("default");
        let selected = choose_nonempty(&primary, &fallback);
        assert_eq!(selected, "default");
    }

    #[test]
    fn kata_12_arc_mutex_shares_one_value() {
        let count = Arc::new(Mutex::new(0_u32));
        let worker_count = Arc::clone(&count);

        let worker = thread::spawn(move || {
            let mut guard = worker_count.lock().unwrap();
            *guard += 1;
        });
        worker.join().unwrap();

        assert_eq!(*count.lock().unwrap(), 1);
    }

    enum Command {
        Describe {
            value: u32,
            reply: mpsc::Sender<String>,
        },
        Stop,
    }

    #[test]
    fn kata_13_channel_command_with_one_shot_reply() {
        let (command_tx, command_rx) = mpsc::channel::<Command>();
        let actor = thread::spawn(move || {
            while let Ok(command) = command_rx.recv() {
                match command {
                    Command::Describe { value, reply } => {
                        let _ = reply.send(format!("value:{value}"));
                    }
                    Command::Stop => break,
                }
            }
        });

        // std mpsc has no dedicated oneshot type, so use a fresh channel once.
        let (reply_tx, reply_rx) = mpsc::channel();
        command_tx
            .send(Command::Describe {
                value: 7,
                reply: reply_tx,
            })
            .unwrap();
        assert_eq!(reply_rx.recv().unwrap(), "value:7");

        command_tx.send(Command::Stop).unwrap();
        actor.join().unwrap();
    }

    // Keep a formatting trait in the exercise so learners can compare a
    // foreign trait implementation with the local traits above.
    impl fmt::Display for SessionId {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.as_str())
        }
    }
}

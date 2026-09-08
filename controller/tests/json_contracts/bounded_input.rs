use std::cell::Cell;
use std::io::Cursor;

use amiss_controller::{ProviderError, decode_bounded_json};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Envelope {
    body: Body,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Body {
    answer: u8,
}

#[test]
fn bounded_capture_checks_limits_before_calling_the_selected_decoder() {
    let input = br#"{"body":{"answer":42}}"#;
    for (declared, maximum, accepted) in [
        (None, input.len(), true),
        (Some(u64::try_from(input.len()).unwrap()), input.len(), true),
        (None, input.len() - 1, false),
        (
            Some(u64::try_from(input.len()).unwrap() + 1),
            input.len(),
            false,
        ),
    ] {
        let calls = Cell::new(0);
        let result = decode_bounded_json(Cursor::new(input), declared, maximum, |bytes| {
            calls.set(calls.get() + 1);
            assert_eq!(bytes, input);
            amiss_wire::read_json::<Envelope>(bytes, u64::MAX)
        });
        if accepted {
            assert_eq!(
                result,
                Ok((
                    Envelope {
                        body: Body { answer: 42 }
                    },
                    input.len()
                ))
            );
            assert_eq!(calls.get(), 1);
        } else {
            assert_eq!(result, Err(ProviderError::InvalidResponse));
            assert_eq!(calls.get(), 0);
        }
    }
}

#[test]
fn selecting_lossless_input_refuses_discarded_fields_and_positional_objects() {
    for input in [
        r#"{"body":{"answer":42,"unknown":true}}"#,
        r#"{"body":{"answer":42},"unknown":true}"#,
        r#"{"body":[42]}"#,
        "[[42]]",
        r#"{"body":{"answer":42,"answer":42}}"#,
        r#"{"body":{"answer":42,"\u0061nswer":42}}"#,
        r#"{"body":{"answer":-0}}"#,
        r#"{"body":{"answer":42}} false"#,
    ] {
        assert_eq!(
            decode_bounded_json(input.as_bytes(), None, input.len(), |bytes| {
                amiss_wire::read_json::<Envelope>(bytes, u64::MAX)
            }),
            Err(ProviderError::InvalidResponse),
            "{input}"
        );
    }
}

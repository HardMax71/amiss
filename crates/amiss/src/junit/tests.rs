#![cfg(test)]

use amiss_wire::json::Value;
use quick_xml::Reader;
use quick_xml::events::Event;

use super::write;

fn row(members: Vec<(&str, Value)>) -> Value {
    Value::object(
        members
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn envelope(status: &str, findings: Vec<Value>, errors: Vec<Value>) -> Value {
    row(vec![(
        "payload",
        row(vec![
            ("result", row(vec![("status", Value::string(status))])),
            ("findings", Value::array(findings)),
            ("errors", Value::array(errors)),
        ]),
    )])
}

fn render(value: &Value) -> String {
    let mut bytes = Vec::new();
    write(value, &mut bytes).expect("write JUnit");
    let mut reader = Reader::from_reader(bytes.as_slice());
    loop {
        match reader.read_event().expect("read produced XML") {
            Event::Eof => break,
            Event::Decl(_)
            | Event::Start(_)
            | Event::End(_)
            | Event::Empty(_)
            | Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::DocType(_)
            | Event::PI(_)
            | Event::GeneralRef(_) => {}
        }
    }
    String::from_utf8(bytes).expect("JUnit is UTF-8")
}

#[test]
fn dispositions_and_analysis_errors_keep_their_report_meaning() {
    let location = row(vec![("path", Value::string("docs/guide.md"))]);
    let finding = |disposition: &str, key: &str| {
        row(vec![
            ("kind", Value::string("explicit-target-missing")),
            ("finding_key", Value::string(key)),
            ("effective_disposition", Value::string(disposition)),
            ("description", Value::string("the target is missing")),
            ("location", location.clone()),
        ])
    };
    let error = row(vec![
        ("code", Value::string("RESOURCE_LIMIT_EXCEEDED")),
        (
            "description",
            Value::string("a resource limit was exceeded"),
        ),
        ("path", Value::Null),
    ]);
    let xml = render(&envelope(
        "incomplete",
        vec![finding("fail", "sha256:a"), finding("warn", "sha256:b")],
        vec![error],
    ));

    assert!(xml.contains("tests=\"3\" failures=\"1\" errors=\"1\""));
    assert!(xml.contains("<failure message=\"the target is missing\""));
    assert!(xml.contains("effective disposition: warn"));
    assert!(xml.contains("<error message=\"a resource limit was exceeded\""));
    assert!(xml.contains("file=\"docs/guide.md\""));
}

#[test]
fn empty_success_and_hostile_xml_scalars_stay_well_formed() {
    let empty = render(&envelope("pass", Vec::new(), Vec::new()));
    assert!(empty.contains("tests=\"1\" failures=\"0\" errors=\"0\""));
    assert!(empty.contains("<testcase classname=\"amiss.result\" name=\"report\" time=\"0\"/>"));

    let failed = render(&envelope("fail", Vec::new(), Vec::new()));
    assert!(failed.contains("<failure message=\"fail\" type=\"amiss-result\">fail</failure>"));
    let incomplete = render(&envelope("incomplete", Vec::new(), Vec::new()));
    assert!(
        incomplete
            .contains("<error message=\"incomplete\" type=\"amiss-result\">incomplete</error>")
    );

    let hostile = row(vec![
        ("kind", Value::string("kind<&\"")),
        ("finding_key", Value::string("key<&\"")),
        ("effective_disposition", Value::string("warn")),
        ("description", Value::string("bad\u{1} description")),
        (
            "location",
            row(vec![("path", Value::string("docs/bad\u{1}.md"))]),
        ),
    ]);
    let xml = render(&envelope("pass", vec![hostile], Vec::new()));
    assert!(xml.contains("kind&lt;&amp;&quot;:key&lt;&amp;&quot;"));
    assert!(xml.contains("bad\u{fffd} description"));
    assert!(!xml.contains(" file="));
}

mod tests;

use std::borrow::Cow;

use amiss_wire::json::Value;
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

use crate::view::View;

enum CaseStatus<'value> {
    Pass,
    Note(String),
    Problem {
        element: &'static str,
        kind: &'value str,
        description: &'value str,
    },
}

pub(crate) fn write(envelope: &Value, output: &mut dyn std::io::Write) -> std::io::Result<()> {
    let payload = View::of(envelope).view("payload");
    let result = payload.view("result");
    let findings = payload.rows("findings");
    let errors = payload.rows("errors");
    let rows = findings.len().saturating_add(errors.len());
    let empty_failure = rows == 0 && result.text("status") == "fail";
    let empty_error = rows == 0 && result.text("status") == "incomplete";
    let tests = rows.max(1).to_string();
    let failures = findings
        .clone()
        .filter(|row| row.text("effective_disposition") == "fail")
        .count()
        .saturating_add(usize::from(empty_failure))
        .to_string();
    let error_count = errors
        .len()
        .saturating_add(usize::from(empty_error))
        .to_string();

    let mut writer = Writer::new(output);
    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))?;

    let counts = [
        ("tests", tests.as_str()),
        ("failures", failures.as_str()),
        ("errors", error_count.as_str()),
        ("time", "0"),
    ];
    let mut suites = BytesStart::new("testsuites");
    suites.extend_attributes(counts);
    writer.write_event(Event::Start(suites))?;

    let mut suite = BytesStart::new("testsuite");
    suite.push_attribute(("name", "amiss"));
    suite.extend_attributes(counts);
    suite.push_attribute(("skipped", "0"));
    writer.write_event(Event::Start(suite))?;

    if rows == 0 {
        let status = match result.text("status") {
            "fail" => CaseStatus::Problem {
                element: "failure",
                kind: "amiss-result",
                description: "the report recorded blocking findings",
            },
            "incomplete" => CaseStatus::Problem {
                element: "error",
                kind: "amiss-result",
                description: "the report recorded an incomplete evaluation",
            },
            _ => CaseStatus::Pass,
        };
        write_case(&mut writer, "amiss.result", "report", None, status)?;
    } else {
        for row in findings {
            let name = format!("{}:{}", row.text("kind"), row.text("finding_key"));
            let disposition = row.text("effective_disposition");
            let status = if disposition == "fail" {
                CaseStatus::Problem {
                    element: "failure",
                    kind: row.text("kind"),
                    description: row.text("description"),
                }
            } else {
                CaseStatus::Note(format!(
                    "effective disposition: {disposition}\n{}",
                    row.text("description")
                ))
            };
            write_case(
                &mut writer,
                "amiss.finding",
                &name,
                text_path(row.view("location")),
                status,
            )?;
        }
        for (index, row) in errors.enumerate() {
            let name = format!("{}:{index}", row.text("code"));
            write_case(
                &mut writer,
                "amiss.analysis-error",
                &name,
                text_path(row),
                CaseStatus::Problem {
                    element: "error",
                    kind: row.text("code"),
                    description: row.text("description"),
                },
            )?;
        }
    }

    writer.write_event(Event::End(BytesEnd::new("testsuite")))?;
    writer.write_event(Event::End(BytesEnd::new("testsuites")))?;
    let output = writer.into_inner();
    output.write_all(b"\n")?;
    output.flush()
}

fn text_path(holder: View<'_>) -> Option<&str> {
    match holder.field("path") {
        Some(Value::String(path)) => Some(path),
        Some(
            Value::Null | Value::Bool(_) | Value::Integer(_) | Value::Object(_) | Value::Array(_),
        )
        | None => None,
    }
}

fn write_case(
    writer: &mut Writer<&mut dyn std::io::Write>,
    classname: &str,
    name: &str,
    file: Option<&str>,
    status: CaseStatus<'_>,
) -> std::io::Result<()> {
    let classname = xml_10(classname);
    let name = xml_10(name);
    let mut case = BytesStart::new("testcase");
    case.push_attribute(("classname", classname.as_ref()));
    case.push_attribute(("name", name.as_ref()));
    case.push_attribute(("time", "0"));
    if let Some(path) = file
        && !path.contains(['\t', '\n', '\r'])
        && let Cow::Borrowed(path) = xml_10(path)
    {
        case.push_attribute(("file", path));
    }

    if matches!(&status, CaseStatus::Pass) {
        writer.write_event(Event::Empty(case))?;
        return Ok(());
    }
    writer.write_event(Event::Start(case))?;
    match status {
        CaseStatus::Pass => {}
        CaseStatus::Note(detail) => {
            let detail = xml_10(&detail);
            writer
                .create_element("system-out")
                .write_text_content(BytesText::new(&detail))?;
        }
        CaseStatus::Problem {
            element,
            kind,
            description,
        } => {
            let kind = xml_10(kind);
            let description = xml_10(description);
            writer
                .create_element(element)
                .with_attribute(("message", description.as_ref()))
                .with_attribute(("type", kind.as_ref()))
                .write_text_content(BytesText::new(&description))?;
        }
    }
    writer.write_event(Event::End(BytesEnd::new("testcase")))
}

fn xml_10(text: &str) -> Cow<'_, str> {
    let allowed = |scalar: char| {
        matches!(scalar, '\t' | '\n' | '\r')
            || matches!(
                u32::from(scalar),
                0x20..=0xd7ff | 0xe000..=0xfffd | 0x0001_0000..=0x0010_ffff
            )
    };
    if text.chars().all(&allowed) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(
            text.chars()
                .map(|scalar| if allowed(scalar) { scalar } else { '\u{fffd}' })
                .collect(),
        )
    }
}

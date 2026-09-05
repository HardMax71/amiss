mod tests;

use std::borrow::Cow;

use amiss_wire::report::Disposition;
use amiss_wire::report::model::{RepoPath, ReportPayload, ReportStatus};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};

enum CaseStatus<'value> {
    Pass,
    Note(String),
    Problem {
        element: &'static str,
        kind: &'value str,
        description: &'value str,
    },
}

pub(crate) fn write(
    payload: &ReportPayload,
    output: &mut dyn std::io::Write,
) -> std::io::Result<()> {
    let result = &payload.result;
    let findings = payload.findings.iter();
    let errors = payload.errors.iter();
    let rows = findings.len().saturating_add(errors.len());
    let empty_failure = rows == 0 && result.status == ReportStatus::Fail;
    let empty_error = rows == 0 && result.status == ReportStatus::Incomplete;
    let tests = rows.max(1).to_string();
    let failures = findings
        .clone()
        .filter(|row| row.effective_disposition == Disposition::Fail)
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
        let status = match result.status {
            ReportStatus::Fail => CaseStatus::Problem {
                element: "failure",
                kind: "amiss-result",
                description: "fail",
            },
            ReportStatus::Incomplete => CaseStatus::Problem {
                element: "error",
                kind: "amiss-result",
                description: "incomplete",
            },
            ReportStatus::Pass => CaseStatus::Pass,
        };
        write_case(&mut writer, "amiss.result", "report", None, status)?;
    } else {
        for row in findings {
            let name = format!("{}:{}", row.kind.as_ref(), row.finding_key);
            let disposition = row.effective_disposition.as_ref();
            let status = if row.effective_disposition == Disposition::Fail {
                CaseStatus::Problem {
                    element: "failure",
                    kind: row.kind.as_ref(),
                    description: &row.description,
                }
            } else {
                CaseStatus::Note(format!(
                    "effective disposition: {disposition}\n{}",
                    row.description
                ))
            };
            write_case(
                &mut writer,
                "amiss.finding",
                &name,
                row.location.path.as_ref().and_then(|path| match path {
                    RepoPath::Text(path) => Some(path.as_str()),
                    RepoPath::Bytes(_) => None,
                }),
                status,
            )?;
        }
        for (index, row) in errors.enumerate() {
            let name = format!("{}:{index}", row.code.as_ref());
            write_case(
                &mut writer,
                "amiss.analysis-error",
                &name,
                row.path.as_ref().and_then(|path| match path {
                    RepoPath::Text(path) => Some(path.as_str()),
                    RepoPath::Bytes(_) => None,
                }),
                CaseStatus::Problem {
                    element: "error",
                    kind: row.code.as_ref(),
                    description: &row.description,
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

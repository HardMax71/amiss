#![cfg(test)]

use amiss_wire::model::RepoPathText;
use amiss_wire::report::model::{
    AnalysisError, AnalysisPhase, RepoPath, ReportEnvelope, ReportPayload, ReportStatus,
};
use amiss_wire::report::{AnalysisErrorCode, Disposition};
use quick_xml::Reader;
use quick_xml::events::Event;

use super::write;

fn render(value: &ReportPayload) -> String {
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
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let payload = &mut report.payload;
    for (finding, disposition) in payload
        .findings
        .iter_mut()
        .zip([Disposition::Fail, Disposition::Warn])
    {
        finding.description = "the target is missing".to_owned();
        finding.effective_disposition = disposition;
        finding.location.path = Some(RepoPath::Text(
            RepoPathText::new("docs/guide.md".to_owned()).unwrap(),
        ));
    }
    payload.errors = vec![AnalysisError {
        code: AnalysisErrorCode::ResourceLimitExceeded,
        description: "a resource limit was exceeded".to_owned(),
        phase: AnalysisPhase::Internal,
        path: None,
        path_bytes_hex: None,
        resource: None,
        configured_limit: None,
        observed_lower_bound: None,
    }];
    payload.result.status = ReportStatus::Incomplete;
    let xml = render(payload);

    assert!(xml.contains("tests=\"3\" failures=\"1\" errors=\"1\""));
    assert!(xml.contains("<failure message=\"the target is missing\""));
    assert!(xml.contains("effective disposition: warn"));
    assert!(xml.contains("<error message=\"a resource limit was exceeded\""));
    assert!(xml.contains("file=\"docs/guide.md\""));
}

#[test]
fn empty_success_and_hostile_xml_scalars_stay_well_formed() {
    let mut report: ReportEnvelope =
        serde_json::from_slice(amiss_fixtures::SCANNER_REPORT).unwrap();
    let payload = &mut report.payload;
    let mut hostile = payload.findings[0].clone();
    payload.findings.clear();
    payload.errors.clear();
    for (status, counts, result) in [
        (
            ReportStatus::Pass,
            "tests=\"1\" failures=\"0\" errors=\"0\"",
            "<testcase classname=\"amiss.result\" name=\"report\" time=\"0\"/>",
        ),
        (
            ReportStatus::Fail,
            "tests=\"1\" failures=\"1\" errors=\"0\"",
            "<failure message=\"fail\" type=\"amiss-result\">fail</failure>",
        ),
        (
            ReportStatus::Incomplete,
            "tests=\"1\" failures=\"0\" errors=\"1\"",
            "<error message=\"incomplete\" type=\"amiss-result\">incomplete</error>",
        ),
    ] {
        payload.result.status = status;
        let xml = render(payload);
        assert!(xml.contains(counts), "{xml}");
        assert!(xml.contains(result), "{xml}");
    }

    hostile.effective_disposition = Disposition::Fail;
    hostile.description = "bad\u{1} description <&\"".to_owned();
    hostile.location.path = Some(RepoPath::Text(
        RepoPathText::new("docs/bad\u{1}.md".to_owned()).unwrap(),
    ));
    payload.findings.push(hostile);
    let xml = render(payload);
    assert!(xml.contains("bad\u{fffd} description &lt;&amp;&quot;"));
    assert!(!xml.contains(" file="));
}

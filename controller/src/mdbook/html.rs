use std::collections::{BTreeMap, BTreeSet};

use html5gum::{Token, Tokenizer};
use url::Url;

use super::MdBookEvidenceError;
use super::context::Page;

const ANCHOR_BYTES: usize = 4_096;

pub(super) fn page_facts(
    html: &[u8],
    route: &str,
    pages: &BTreeMap<String, Page>,
    anchor_count: &mut usize,
    href_count: &mut usize,
) -> Result<(Vec<String>, Vec<String>), MdBookEvidenceError> {
    let mut anchors = BTreeSet::new();
    let mut base_href = None;
    let mut hrefs = BTreeSet::new();
    for token in Tokenizer::new(html) {
        let token = match token {
            Ok(token) => token,
            Err(never) => match never {},
        };
        match token {
            Token::StartTag(tag) => {
                let name = tag.name.as_ref();
                let is_base = name == b"base".as_slice();
                let is_link = name == b"a".as_slice() || name == b"area".as_slice();
                for (name, value) in tag.attributes {
                    if name.as_ref() == b"id".as_slice() {
                        let anchor = String::from_utf8(value.value.0)
                            .map_err(|_defect| MdBookEvidenceError::Anchor)?;
                        if anchor.is_empty()
                            || anchor.len() > ANCHOR_BYTES
                            || anchor.chars().any(char::is_control)
                        {
                            return Err(MdBookEvidenceError::Anchor);
                        }
                        if anchors.insert(anchor) {
                            *anchor_count = anchor_count
                                .checked_add(1)
                                .filter(|count| {
                                    *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT
                                })
                                .ok_or(MdBookEvidenceError::Anchor)?;
                        }
                    } else if name.as_ref() == b"href".as_slice()
                        && ((is_base && base_href.is_none()) || is_link)
                    {
                        let href = String::from_utf8(value.value.0)
                            .map_err(|_defect| MdBookEvidenceError::Navigation)?;
                        if is_base && base_href.is_none() {
                            base_href = Some(href);
                        } else if is_link && hrefs.insert(href) {
                            *href_count = href_count
                                .checked_add(1)
                                .filter(|count| {
                                    *count <= amiss_wire::semantic::SEMANTIC_OBSERVATIONS_LIMIT
                                })
                                .ok_or(MdBookEvidenceError::Navigation)?;
                        }
                    }
                }
            }
            Token::EndTag(_)
            | Token::String(_)
            | Token::Comment(_)
            | Token::Doctype(_)
            | Token::Error(_) => {}
        }
    }

    let document = Url::parse(&format!("https://amiss.invalid{route}"))
        .map_err(|_defect| MdBookEvidenceError::Navigation)?;
    let base = base_href
        .and_then(|href| document.join(&href).ok())
        .unwrap_or(document);
    let mut destinations = BTreeSet::new();
    for href in hrefs {
        let Ok(destination) = base.join(&href) else {
            continue;
        };
        if destination.scheme() == "https"
            && destination.host_str() == Some("amiss.invalid")
            && destination.port().is_none()
            && destination.username().is_empty()
            && destination.password().is_none()
            && pages.contains_key(destination.path())
        {
            destinations.insert(destination.path().to_owned());
        }
    }
    Ok((
        anchors.into_iter().collect(),
        destinations.into_iter().collect(),
    ))
}

pub(super) fn reachable_sources(
    entrypoint: &str,
    links: &BTreeMap<String, Vec<String>>,
    pages: &BTreeMap<String, Page>,
) -> Result<Vec<String>, MdBookEvidenceError> {
    let mut pending = vec![entrypoint.to_owned()];
    let mut reached = BTreeSet::new();
    while let Some(route) = pending.pop() {
        if !reached.insert(route.clone()) {
            continue;
        }
        let destinations = links.get(&route).ok_or(MdBookEvidenceError::Navigation)?;
        pending.extend(destinations.iter().rev().cloned());
    }
    let mut sources = BTreeSet::new();
    for route in reached {
        let page = pages.get(&route).ok_or(MdBookEvidenceError::Navigation)?;
        sources.extend(page.source.iter().cloned());
    }
    Ok(sources.into_iter().collect())
}

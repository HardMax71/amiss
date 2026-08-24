use amiss_wire::controls::TargetKind;
use amiss_wire::uri::decode_fragment;

use crate::Error;
use crate::semantic::{SiteRoute, View};

use super::syntax::split_components;
use super::{Resolution, Resolver, lookup};

pub(super) fn resolve(
    resolver: &mut Resolver<'_>,
    semantic: View<'_>,
    destination: &str,
    is_image: bool,
) -> Result<Option<Resolution>, Error> {
    let Some(routes) = semantic.routes else {
        return Ok(None);
    };
    let (route, _query, fragment) = split_components(destination);
    let Some(SiteRoute::Unique { source, anchors }) = routes.get(route) else {
        return Ok(None);
    };
    if is_image || !resolver.snapshot.is_scanned_structured(source) {
        return Ok(None);
    }
    if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
        let Some(decoded) = decode_fragment(&fragment) else {
            return Ok(None);
        };
        if anchors
            .binary_search_by(|anchor| anchor.as_str().cmp(&decoded))
            .is_err()
        {
            return Ok(None);
        }
    }
    let resolution = lookup(resolver, source, TargetKind::Blob, None, None, None)?;
    Ok(matches!(&resolution, Resolution::Resolved(_)).then_some(resolution))
}

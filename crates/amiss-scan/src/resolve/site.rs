use amiss_wire::controls::{GitMode, TargetKind};
use amiss_wire::resolution::ExternalReference;

use crate::Error;
use crate::discovery::Located;
use crate::semantic::{SiteClaim, SitePageBacking, SiteRoute, SiteTarget, View, fragment_target};

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
    let (page, fragment) = match routes.get(route) {
        Some(
            page @ SiteRoute::Unique(SiteClaim {
                target: SiteTarget::Page { .. },
                ..
            }),
        ) => (Some(page), fragment.as_deref()),
        Some(SiteRoute::Unique(SiteClaim {
            target:
                SiteTarget::Redirect {
                    destination,
                    fragment: redirected_fragment,
                },
            ..
        })) => (
            routes.get(destination),
            redirected_fragment.as_deref().or(fragment.as_deref()),
        ),
        Some(SiteRoute::Ambiguous { .. }) | None => (None, None),
    };
    let Some(SiteRoute::Unique(SiteClaim {
        source,
        target: SiteTarget::Page { backing, anchors },
        ..
    })) = page
    else {
        return Ok(None);
    };
    if is_image {
        return Ok(None);
    }
    if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty())
        && !fragment_target(anchors, fragment)
    {
        return Ok(None);
    }
    match backing {
        SitePageBacking::Repository => {
            let Some(source) = source else {
                return Ok(None);
            };
            if !resolver.snapshot.is_scanned_structured(source) {
                return Ok(None);
            }
            let resolution = lookup(resolver, source, TargetKind::Blob, None, None, None)?;
            Ok(matches!(&resolution, Resolution::Resolved { .. }).then_some(resolution))
        }
        SitePageBacking::Generated => Ok(source
            .as_ref()
            .is_none_or(|source| {
                matches!(
                    resolver.snapshot.locate(source),
                    Some(Located::Entry(
                        GitMode::RegularFile | GitMode::ExecutableFile,
                        _
                    ))
                )
            })
            .then_some(Resolution::External(ExternalReference::SiteBuild))),
    }
}

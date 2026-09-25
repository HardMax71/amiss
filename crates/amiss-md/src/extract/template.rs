use amiss_wire::extraction::SourceConstruct;

use super::PreprocessorInclude;

/// The spans Liquid `raw` blocks cover, `{% raw %}` to `{% endraw %}`, where a
/// tag is shown rather than evaluated. One left open runs to the end.
pub(super) fn raw_blocks(suffix: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut opened: Option<usize> = None;
    let mut from = 0_usize;
    while let Some((start, end, body)) = next_tag(suffix, from, suffix.len(), "{%", "%}") {
        let name = body.split_whitespace().next().unwrap_or_default();
        if opened.is_none() && name == "raw" {
            opened = Some(start);
        } else if let Some(open) = opened.filter(|_| name == "endraw") {
            found.push((open, end));
            opened = None;
        }
        from = end;
    }
    found.extend(opened.map(|open| (open, suffix.len())));
    found
}

/// Every destination a site generator's template writes from one text run,
/// which Markdown never reads as a link since the template holds spaces:
/// Jekyll's `{% link path %}` and `{% post_url name %}`, a quoted URL through
/// `relative_url` or `absolute_url`, and Hugo's `ref` and `relref` shortcodes.
/// The template opens in the run and closes on the same line, since Markdown
/// reads `<relref page >` inside one as an HTML tag of its own. A template
/// inside a Liquid `raw` block is shown rather than evaluated.
pub(super) fn destinations(
    suffix: &str,
    span: (usize, usize),
    raw: &[(usize, usize)],
) -> Vec<PreprocessorInclude> {
    let mut found = Vec::new();
    let mut at = span.0;
    while let Some(open) = suffix
        .get(at..span.1)
        .and_then(|rest| rest.find('{'))
        .map(|offset| at.saturating_add(offset))
    {
        let line_end = suffix
            .get(open..)
            .and_then(|rest| rest.find('\n'))
            .map_or(suffix.len(), |offset| open.saturating_add(offset));
        let rest = suffix.get(open..line_end).unwrap_or_default();
        let Some((opener, closer)) = [("{{<", ">}}"), ("{{%", "%}}"), ("{%", "%}"), ("{{", "}}")]
            .into_iter()
            .find(|(opener, _)| rest.starts_with(opener))
        else {
            at = open.saturating_add(1);
            continue;
        };
        let Some((start, end, body)) = next_tag(suffix, open, line_end, opener, closer) else {
            break;
        };
        let shown = raw.iter().any(|(from, to)| *from <= start && start < *to);
        if let Some((construct, target)) = destination(opener, body).filter(|_| !shown) {
            found.push(PreprocessorInclude {
                construct,
                raw: suffix.get(start..end).unwrap_or_default().to_owned(),
                target,
                span: (start, end),
            });
        }
        at = end;
    }
    found
}

/// The construct and target one template body names, when it is one of the
/// destination forms and its argument is a literal rather than an expression.
fn destination(opener: &str, body: &str) -> Option<(SourceConstruct, String)> {
    if opener == "{{" {
        let (value, filter) = body.split_once('|')?;
        if !matches!(filter.trim(), "relative_url" | "absolute_url") {
            return None;
        }
        let value = quoted(value.trim())?;
        let route = if value.starts_with('/') || value.contains("://") {
            value.to_owned()
        } else {
            format!("/{value}")
        };
        return Some((SourceConstruct::MarkdownLiquidUrl, route));
    }
    let (name, argument) = body.split_once(char::is_whitespace)?;
    let argument = argument.trim();
    if argument.is_empty() || argument.contains(['{', '=']) {
        return None;
    }
    let literal = quoted(argument).unwrap_or(argument);
    match (opener, name) {
        ("{%", "link") => Some((SourceConstruct::MarkdownLiquidLink, literal.to_owned())),
        ("{%", "post_url") => Some((
            SourceConstruct::MarkdownLiquidLink,
            format!("_posts/{}", literal.trim_start_matches('/')),
        )),
        ("{{<" | "{{%", "ref" | "relref") => Some((
            SourceConstruct::MarkdownHugoRef,
            quoted(argument)
                .or_else(|| argument.split_whitespace().next())
                .unwrap_or(argument)
                .to_owned(),
        )),
        _ => None,
    }
}

/// One template from the first `open` at or after `from` to the `close` after
/// it, before `limit`: its span and its body, whitespace-control dashes off.
/// An opener nothing closes ends the reading, since no later one can close
/// either, which keeps the scan linear.
fn next_tag<'a>(
    text: &'a str,
    from: usize,
    limit: usize,
    open: &str,
    close: &str,
) -> Option<(usize, usize, &'a str)> {
    let start = from.saturating_add(text.get(from..limit)?.find(open)?);
    let body_start = start.saturating_add(open.len());
    let body_end = body_start.saturating_add(text.get(body_start..limit)?.find(close)?);
    let body = text
        .get(body_start..body_end)?
        .trim_start_matches('-')
        .trim_end_matches('-')
        .trim();
    Some((start, body_end.saturating_add(close.len()), body))
}

/// A string literal's text, in double or single quotes or Hugo's backticks.
fn quoted(value: &str) -> Option<&str> {
    ['"', '\'', '`'].into_iter().find_map(|quote| {
        value
            .strip_prefix(quote)?
            .strip_suffix(quote)
            .filter(|inner| !inner.contains(quote))
    })
}

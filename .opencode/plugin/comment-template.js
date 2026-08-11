const STATUS = /^STATUS:[ \t]*(ok|issues)[ \t]*\r?\n/i
const DONE = /^[ \t]*WHAT WAS DONE[ \t]*$/im
const REVIEW = /^[ \t]*review:[ \t]*(\S+)[ \t]*$/im

function template(text) {
  const status = text.match(STATUS)
  if (!status) return text
  let body = text.slice(status[0].length)

  let review = ""
  const reviewMatch = body.match(REVIEW)
  if (reviewMatch) {
    review = reviewMatch[1]
    body = body.replace(REVIEW, "")
  }

  let done = ""
  const doneMatch = body.match(DONE)
  if (doneMatch) {
    done = body.slice(doneMatch.index + doneMatch[0].length).trim()
    body = body.slice(0, doneMatch.index)
  }
  body = body.trim()

  const kind = status[1].toLowerCase() === "ok" ? "TIP" : "WARNING"
  const split = body.indexOf("\n\n")
  const verdict = split === -1 ? body : body.slice(0, split)
  const rest = split === -1 ? "" : body.slice(split + 2).trim()

  const parts = []
  parts.push(`> [!${kind}]\n` + verdict.split("\n").map((l) => "> " + l).join("\n"))
  if (rest) parts.push(rest)
  if (done) parts.push(`<details><summary>What was done</summary>\n\n${done}\n\n</details>`)
  const tail = review ? `\n\n[review](${review})` : ""
  parts.push(`<details><summary>Session details</summary>${tail}`)
  return parts.join("\n\n")
}

export const CommentTemplate = async () => {
  if (!process.env.GITHUB_RUN_ID) return {}
  return {
    "experimental.text.complete": async (_input, output) => {
      output.text = template(output.text)
    },
  }
}

export const _test = { template }

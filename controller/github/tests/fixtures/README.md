The Git commit responses were captured read-only on 2026-09-09 with
`X-GitHub-Api-Version: 2022-11-28`. They are unmodified JSON responses, including the
signature and signed payload where present. Tests run offline against these fixed bytes.

The [unsigned commit](https://api.github.com/repos/HardMax71/amiss/git/commits/9cefdc3c43f7f5c2b2da9f4bb84e1170842b668e)
has one parent and null signature metadata. The
[signed merge](https://api.github.com/repos/github/rest-api-description/git/commits/3cef12e8a02d612ad032473d4fb87266f2befeae)
has two parents and populated verification metadata. The model follows the
[GitHub Git commit contract](https://docs.github.com/en/rest/git/commits#get-a-commit-object).

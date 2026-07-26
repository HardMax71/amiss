# Awesome Gitea

[![Awesome](https://awesome.re/badge-flat.svg)](https://awesome.re)
[![Contribution%20Guide](https://img.shields.io/badge/-Contribution%20Guide-informational?style=flat)](contributing.md)

A curated list of awesome projects related to Gitea and its soft-fork instances.

## Legend

Entries are grouped by maintenance status:

- **Active projects** — maintained or in regular use.
- **Unmaintained** — previously marked as not actively maintained; they may still work.
- **Archived** — repository is archived on its forge (read-only) or explicitly sunset.

## Contents

- [Awesome Gitea](#awesome-gitea)
  - [Legend](#legend)
  - [Contents](#contents)
  - [Active projects](#active-projects)
    - [Actions](#actions)
    - [Applications](#applications)
      - [Bot](#bot)
      - [Command Line](#command-line)
      - [DevOps](#devops)
      - [Mobile](#mobile)
      - [Web Hosting](#web-hosting)
    - [Migration](#migration)
    - [Organizations](#organizations)
      - [Open Registration](#open-registration)
      - [For internal use](#for-internal-use)
    - [Packages](#packages)
    - [Plugins](#plugins)
    - [Scripts](#scripts)
    - [SDK](#sdk)
    - [Templates](#templates)
    - [Themes](#themes)
      - [Light](#light)
      - [Dark](#dark)
    - [Project Management](#project-management)
  - [Unmaintained](#unmaintained)
    - [Applications](#applications-1)
      - [Bot](#bot-1)
      - [Command Line](#command-line-1)
      - [DevOps](#devops-1)
      - [Mobile](#mobile-1)
      - [Panel](#panel)
      - [Web Hosting](#web-hosting-1)
    - [Migration](#migration-1)
    - [Packages](#packages-1)
    - [Package Management](#package-management)
    - [Plugins](#plugins-1)
    - [Scripts](#scripts-1)
    - [SDK](#sdk-1)
    - [Themes](#themes-1)
      - [Light](#light-1)
      - [Dark](#dark-1)
  - [Archived](#archived)
    - [Applications](#applications-2)
      - [Bot](#bot-2)
      - [Panel](#panel-1)
    - [Packages](#packages-2)
    - [SDK](#sdk-2)
    - [Themes](#themes-2)
    - [Workflow Tools](#workflow-tools)

## Active projects

### Actions

- [gitea-publish-generic-packages](https://github.com/VAllens/gitea-publish-generic-packages) - An action to support publishing generic packages to Gitea. `MIT` `JavaScript`
- [gitea-release-action](https://gitea.com/actions/gitea-release-action) - An action to support publishing releases to Gitea. `MIT` `JavaScript`
- [gitea-release-please](https://github.com/marketplace/actions/gitea-release-please-action) - An action to support Automated releases with Conventional Commit Messages. `Apache-2.0` `TypeScript`

### Applications

#### Bot

- [gopher-bot](https://github.com/nfort/gopher-bot) - Bot for checking golang code `MIT` `Go`
- [sq-bot](https://codeberg.org/justusbunsi/gitea-sonarqube-bot) - Bot for decorating Gitea pull requests with SonarQube analysis details. `MIT` `Go`

#### Command Line

- [changelog](https://gitea.com/gitea/changelog) - Generate changelog of gitea repository. `MIT` `Go`
- [gcli](https://github.com/herrhotzenplotz/gcli) - A CLI for Gitea, Gitlab and Github written in C `BSD-2-Clause` `C`
- [grp](https://github.com/feraxhp/grp) - A cli tool to interact with github, gitea and local repositories written in rust. `MIT` `Rust`
- [tea](https://gitea.com/gitea/tea) - A command line tool to interact with Gitea servers. `MIT` `Go`

#### Cloud Hosting

[![Deploy with Zenith](https://cdn.zenith.hosting/buttons/deploy-with-zenith.svg)](https://zenith.hosting/host/gitea?ref=gh)

Zenith Hosting offers Gitea as a one-click deployment option on the platform. They handle storage, backups, security and more.

#### DevOps

- [actions runner](https://gitea.com/ChristopherHX/actions_runner) - Use the actions/runner developed by GitHub with Gitea Actions. `MIT` `Go`
- [agola](https://github.com/agola-io/agola) - Agola: CI/CD Redefined. Built-in Gitea support.(see [``docs``](https://agola.io/tryit/#test-using-a-local-gitea-instance)) `Apache-2.0` `Go`
- [ai-git-bot](https://github.com/tmseidel/ai-git-bot) - AI-Git Bot. Self hostable gateway for DevOps automations directly in Gitea. `MIT` `Java`
- [appveyor](https://www.appveyor.com/) - Gitea receives first-class support in AppVeyor CI.
- [buildbot-gitea](https://github.com/lab132/buildbot-gitea) - Buildbot plugin for integration with gitea. `MIT` `Python`
- [Concourse](https://www.concourse-ci.org/) - partially can be integrated with Gitea.
- [dex](https://github.com/dexidp/dex) - Dex is a federated OpenID Connect provider. Built-in Gitea support. `Apache-2.0` `Go`
- [drone](https://github.com/drone/drone) - Drone is a Container-Native, Continuous Delivery Platform. Built-in Gitea support. (see [docs](https://docs.drone.io/server/provider/gitea/)) `Apache-2.0` `Go/TypeScript`
- [GARM](https://github.com/cloudbase/garm) - Multi-cloud, auto-scaling manager for GitHub Actions & Gitea self-hosted runners with pluggable providers.
- [ghorg](https://github.com/gabrie30/ghorg) - Quickly clone an entire org/users repositories into one directory - Supports Gitea, GitHub, GitLab, and more. `Apache-2.0` `Go`
- [gickup](https://github.com/cooperspencer/gickup) - Backup tool for repositories. `Apache-2.0` `Go`
- [gitea-notification-hub](https://github.com/vinnyy-afk/Gitea-Notifiction-Hub) - Receive Gitea webhooks and send personalized Slack notifications with real @mentions for PRs, issues, and comments. `AGPL-3.0` `Go`
- [Jenkins](https://github.com/jenkinsci/gitea-plugin) - Gitea plugin for jenkins. `MIT` `Java`
- [mvoCI](https://codeberg.org/snaums/mvoCI) - very simple Continuous Integration Server written in go. Built-in Gitea support. `GPL-3.0` `Go`
- [Renovate](https://github.com/renovatebot/renovate) - Gitea compatible configurable universal dependability update tool `AGPL-3.0` `TypeScript`
- [soba](https://github.com/jonhadfield/soba) - scheduled backups of user/organization Gitea repositories with change detection. `MIT` `Go`
- [Tea Runner](https://github.com/DavesCodeMusings/tea-runner) - A minimalist Python Flask app that uses Gitea webhooks to perform actions. `BSD-2-Clause` `Python`
- [terraform-provider-gitea](https://gitea.com/gitea/terraform-provider-gitea) - Terraform provider to manage Gitea infrastructure as code. `MIT` `Go`
- [webhook](https://github.com/adnanh/webhook) - webhook is a lightweight incoming webhook server to run shell commands. Useful for running Continuous Deployment pipeline steps. `MIT` `Go`
- [webhookd](https://github.com/ncarlier/webhookd) - A very simple webhook server launching shell scripts. Useful for running Continuous Deployment pipeline steps. `MIT` `Go`
- [woodpecker](https://github.com/woodpecker-ci/woodpecker) - An opinionated fork of the Drone CI system. Built-in Gitea support. (see [docs](https://woodpecker-ci.org/docs/administration/configuration/forges/gitea)) `Apache-2.0` `Go`
- [yojo](https://sr.ht/~emersion/yojo/) - A CI bridge from Gitea to SourceHut.

#### Mobile

- [GitNex](https://codeberg.org/gitnex/GitNex) - Android client for Gitea. `GPL-3.0` `Java`

#### Web Hosting

- [Caddy Gitea Plugin (d7z-project/caddy-gitea-pages)](https://github.com/d7z-project/caddy-gitea-pages) - A simple Gitea Pages plugin that is compatible with Github Pages, supports custom domains, and can be published using Gitea Actions. `Apache-2.0` `Go`
- [Meli](https://github.com/getmeli/meli) - Open source platform built for deploying static sites and frontend applications.
- [Codeberg Pages](https://codeberg.org/Codeberg/pages-server) - Static Pages Server, Gitea equivalent of GitHub Pages: Can serve static webpages on custom domains, including caching, and much more `EUPL-1.2` `Go`
- [pages-server](https://git.mills.io/prologic/pages-server) - A simple server for serving up static pages for Gitea A Gitea Pages server ala Github pages. `MIT` `Go`
- [Pages Server](https://github.com/d7z-project/gitea-pages) - Another opinionated gitea pages project. GitHub-compatible with custom template rendering support. `Apache-2.0` `Go`
- [gitea-pages](https://github.com/deadnews/gitea-pages) - A static pages server for Gitea with minimal dependencies. `MIT` `Go`

### Migration

- [BitbucketServer2Gitea](https://github.com/appleboy/BitbucketServer2Gitea) - A command line tool build with Golang to migrate a Bitbucket Server (Stash) Project to Gitea. `MIT` `Go`
- [Bitbucket2Gitea](https://github.com/sIspravnikov/BitbucketToGitea) - A python3 script to migrate all projects and repositories from Atlassian BitBucket to Gitea. `GPL-3.0` `Python`
- [gitlab2gitea](https://github.com/cornelk/gitlab2gitea) - A command line tool build with Golang to migrate a GitLab project to Gitea. `MIT` `Go`

### Organizations

#### Open Registration

- [OpenDev](https://opendev.org/) - A space for collaborative Open Source software development.
- [RadioRepo](https://repo.radio/) - The home of software development for the Amateur Radio Community.

#### For internal use

- [Blender](https://projects.blender.org) - The Blender Projects portal where all the (Blender) official initiatives are coordinated and managed.
- [openSUSE](https://src.opensuse.org/) - openSUSE Gitea
- [FSFE](https://git.fsfe.org/) - Git @ Free Software Foundation Europe

### Packages

- [docker-openshift-gitea](https://github.com/wkulhanek/docker-openshift-gitea) - Gitea container for OpenShift `N/A` `Shell/Dockerfile`
- [Gitea Debian/Ubuntu packages](https://gitlab.com/packaging/gitea) - Debian/Ubuntu packages `MIT` `Shell`
- [gitea_yhn](https://github.com/YunoHost-Apps/gitea_ynh) - Gitea package for YunoHost `MIT` `Shell`
- [helm-chart](https://gitea.com/gitea/helm-chart) - Official Gitea Helm Chart `MIT` `Helm`
- [Raspbian Addons](https://raspbian-addons.org) - an APT repository for Raspberry Pi which includes up-to-date gitea packages.
- [SynoCommunity Gitea](https://synocommunity.com/package/gitea) - Synology Gitea Package

### Plugins

- [git-kanban-enhanced-extension](https://github.com/funktechno/git-kanban-enhanced-extension) - chrome extension to add additional kanban project planning to git hosting: github.com, gitlab.com, gitea.io, bitbucket.org `MIT` `TypeScript`
- [Gitea Extension for Visual Studio](https://marketplace.visualstudio.com/items?itemName=MysticBoy.GiteaExtensionforVisualStudio) - A Visual Studio Extension that brings the Gitea Flow into Visual Studio. `MIT` `C#`
- [gitea-vs-extension](https://github.com/bircni/gitea-vs-extension) - Gitea Extension for VSCode (Comments, Actions, Repository Management) `MIT` `TypeScript`
- [Gitea-VSCode](https://marketplace.visualstudio.com/items?itemName=ijustdev.gitea-vscode) - Gitea Issue Tracker for vs-code `MIT` `TypeScript`
- [Gitea](https://github.com/LeonDevLifeLog/gitea-idea-plugin) - plugin for JetBrains IDEs (Idea, Android Studio, etc.). `MIT` `Kotlin`
- [Gitea-Anchorpoint](https://docs.anchorpoint.app/docs/1-overview/integrations/gitea) - Gitea integration plugin for an artist friendly Git client.
- [Gitea ONLYOFFICE Bridge](https://github.com/NovichekLIS/Gitea-onlyoffice) - ONLYOFFICE document preview and editing bridge for Gitea. `MIT` `Python/JavaScript`
- [Gitea CAD Viewer](https://github.com/NovichekLIS/Gitea_CAD) - Read-only DWG and DXF preview integration for Gitea. `MIT` `JavaScript`
- [gitea-conventional-comments-button](https://github.com/sebastian-sauer/gitea-conventional-comments-button) - A browser extension to add buttons for [conventional comments](https://conventionalcomments.org/) to review comment boxes. `MIT` `JavaScript`
- [picgo-plugin-gitea-uploader](https://github.com/GeorgeHu6/picgo-plugin-gitea-uploader) - A PicGo uploader plugin that stores images in a Gitea repository through the Gitea REST API. `MIT` `TypeScript`

### Scripts

- [nodiscc.xsrv.gitea](https://github.com/nodiscc/xsrv/tree/master/roles/gitea) - Ansible role to install and configure Gitea `GPL-3.0` `Ansible`
- [nodiscc.xsrv.gitea_act_runner](https://github.com/nodiscc/xsrv/tree/master/roles/gitea_act_runner) - Ansible role to install and configure `act_runner` `GPL-3.0` `Ansible`
- [solarchemist/gitea](https://codeberg.org/ansible/gitea) - Ansible role to install and configure multiple Gitea instances on the same host. `GPL-3.0` `Ansible`

### SDK

- [gitea-js](https://github.com/anbraten/gitea-js) - Gitea client in Typescript for browsers and Node.JS ([npm](https://www.npmjs.com/package/gitea-js)) ([docs](https://anbraten.github.io/gitea-js/)) `MIT` `TypeScript`
- [Golang SDK](https://gitea.com/gitea/go-sdk) - Official Golang SDK for gitea. `MIT` `Go`
- [py-gitea](https://github.com/Langenfeld/py-gitea/) - A very simple API client for Gitea > 1.16.1 `MIT` `Python`
- [tea4j-autodeploy](https://codeberg.org/gitnex/tea4j-autodeploy) - Swagger-generated Java library which uses Retrofit to access the Gitea API `GPL-3.0` `Java`
- [java-gitea-api](https://github.com/le-shi/java-gitea-api) - Swagger generated api for Gitea `MIT` `Java`

### Templates

- [GiteaMailTemplates](https://github.com/KenanZhu/GiteaMailTemplates) - A curated collection of 110 professionally designed email templates for self-hosted Gitea — 10 visual styles, drop-in ready. `MIT` `Go`

### Themes

- [Catppuccin](https://github.com/catppuccin/gitea) - Soothing pastel theme for Gitea `MIT` `CSS`
- [GitHub Themes](https://github.com/lutinglt/gitea-github-theme) - Exquisite GitHub style Gitea themes. `Apache-2.0` `TypeScript`
- [Lugit Themes](https://github.com/lucas-labs/gitea-lugit-theme) - Light-Dark themes inspired by Github and Catppuccin `MIT` `CSS`
- [pat-s/GitHub](https://codeberg.org/pat-s/gitea-github-theme) - Opinionated GitHub-inspired light and dark themes `MIT` `CSS`
- [Sainnhe's Theme Pack](https://git.sainnhe.dev/sainnhe/gitea-themes) - Port of some editor themes `GPL-3.0` `CSS`
- [theme.park](https://docs.theme-park.dev/themes/gitea) - Rich theme suite that includes Gitea `MIT` `CSS`

#### Light

- [Light Blue](https://github.com/sIspravnikov/gitea-lightblue) - Light blue theme inspired by Bitbucket `GPL-3.0` `CSS`

#### Dark

- [Bthree Dark](https://projects.blender.org/infrastructure/gitea-custom) - A dark theme created and used by the Blender Project. `N/A` `CSS`
- [Dark Arc](https://github.com/Jieiku/theme-dark-arc-gitea) - Dark theme with high contrast, based on arc-green. `MIT` `CSS`
- [Dark Blue](https://gitea.artixlinux.org/artix/gitea-dark-blue) - The dark blue Gitea theme used on [https://gitea.artixlinux.org](https://gitea.artixlinux.org) `MIT` `CSS`
- [Earl Grey](https://github.com/Troplo/earl-grey) - An elegant dark theme for Gitea with blue as the primary color. `MIT` `CSS`
- [GitHub](https://github.com/Rainnny7/gitea-github-theme) - A theme to make Gitea look and feel like GitHub. `MIT` `CSS`
- [One Dark](https://git.tjdev.de/tjdev/gitea-theme-one-dark) - One Dark theme used on [git.tjdev.de](https://git.tjdev.de) `MIT` `CSS`
- [Tangerine Dream](https://github.com/jager012/tangerine-dream) - Tangerine dark theme for Gitea `N/A` `CSS`

### Project Management

- [JetBrains YouTrack](https://www.jetbrains.com/help/youtrack/standalone/integration-with-gitea.html) - A web-based issue tracking and project management platform
- [Jira Gitea Connector](https://github.com/alphabox/jgc) - A middleware application that acts as a bridge between Gitea and the [GitHub for Atlassian Plugin](https://marketplace.atlassian.com/apps/1219592/github-for-atlassian).

## Unmaintained

### Applications

#### Bot

- [issue-bot](https://git.meli.delivery/meli/issue-bot) - Bot for mailing list mirroring of Gitea issues. Allow people to submit issues on repositories using only e-mail without signing up. [github read-only mirror](https://github.com/meli/issue-bot) `N/A` `Rust`
- [staletea](https://gitea.com/jonasfranz/staletea) - StaleBot for Gitea. `GPL-3.0` `Go`

#### Command Line

- [gitea-cli](https://github.com/bashup/gitea-cli) - Extensible, configurable command-line API client for gitea and gogs. `MIT` `Shell`
- [gitea-installer](https://github.com/uvulpos/gitea-installer) - a simple ubuntu native installer script `MIT` `Shell`
- [makepr](https://github.com/hrgdavor/makepr) - Quickly open url to start PR process with current branch. `MIT` `JavaScript`
- [sip](https://gitea.com/jolheiser/sip) - A prompt-based command line tool to interact with Gitea servers. `MIT` `Go`

#### DevOps

- [AWS Cloud Integration(webhook-to-s3)](https://github.com/leonli/webhook-to-s3) - Gitea Webhook integration with AWS CodePipeline and CodeBuild by automatically uploading the archive to AWS S3. `Apache-2.0` `JavaScript`
- [buildkite-connector](https://github.com/techknowlogick/gitea-buildkite-connector) - Connect Gitea & Buildkite. `MIT` `Go`
- [JayporeCi](https://github.com/theSage21/jaypore_ci) - Self hosted CI tightly integrated with gitea `MIT` `Python`
- [Metroline](https://github.com/metroline/metroline) - Metroline is a Continuous Integration and Delivery platform built with Docker, Node, React and MongoDB, natively compatible with Gitea. `GPL-3.0` `TypeScript`

#### Mobile

- [GitTouch](https://github.com/git-touch/git-touch) - Open source mobile client for GitHub, GitLab, Bitbucket and Gitea, built with Flutter `Apache-2.0` `Dart`

#### Panel

- [GiteaPanel](https://github.com/sashaoli/GiteaPanel) - Manage the local Gitea server from the tray. `MIT` `Pascal`
- [US/GiteaPanel](https://github.com/kerwin612/us-giteapanel) - A Gitea shortcut panel built based on UserScript. `MIT` `JavaScript`

#### Web Hosting

- [Caddy Gitea Plugin (42wim/caddy-gitea)](https://github.com/42wim/caddy-gitea) - Caddy2 plugin enables GitHub pages-like features in Gitea, requiring a wildcard CNAME to your Gitea host. `Apache-2.0` `Go`

### Migration

- [github2gitea](https://gitea.com/yige/github2gitea) - A python script to migrate Github repositories Gitea with issues/prs/wiki and etc. `MIT` `Python`
- [Gogs2Gitea](https://github.com/lesh59/Gogs2Gitea) - A SQL script and process (README) to migrate directly from Gogs 0.12.3 to Gitea 1.12.5 / 1.12.6 in MySQL/MariaDB and maybe other DB's. `GPL-3.0` `SQL`
- [jira2giteaMySql](https://github.com/juangarcia06/jira2giteaMySql) - Jira Issues to Gitea (with MySql) `MIT` `C#`

### Packages

- [gitea-chocolatey](https://github.com/doggy8088/gitea-chocolatey) - Chocolatey package for gitea `MIT` `PowerShell`
- [synology-gitea-jboxberger](https://github.com/jboxberger/synology-gitea-jboxberger) - Synology Gitea Package `MIT` `Shell`

### Package Management

- [Acappella](https://github.com/sitelease/acappella) - Private Composer Repository for Gitea `MIT` `PHP`

### Plugins

- [git-master](https://github.com/ineo6/git-master) - Git Master Extension for git file tree, support GitHub, GitLab, Gitee, Gitea `MIT` `JavaScript`
- [gitea-comment-plugin](https://github.com/TsakiDev/gitea-comment) - A Drone plugin to post comments on a Gitea Pull Request. `GPL-3.0` `C#`
- [gitea-kanban](https://github.com/qontu/gitea-kanban) - Kanban for Gitea done in Vue `MIT` `TypeScript`
- [gitea-preview](https://github.com/pacman-ghost/gitea-preview) - Preview files (including HTML) directly from a Gitea repo. `Apache-2.0` `JavaScript/PHP`
- [intellij-gitea-plugin](https://github.com/e1fueg0/intellij-gitea-plugin) - Gitea issue tracker integration plugin for Jetbrains IDE platform. `MIT` `Java`
- [redmine_merge_request_links](https://github.com/tf/redmine_merge_request_links#gitea) - Gitea pull request integration for Redmine issue tracker. `MIT` `Ruby`

### Scripts

- [docker-gitea](https://gitea.com/jwobith/docker-gitea) - Docker Gitea Service `MIT` `Docker Compose`

### SDK

- [Dart](https://pub.dev/packages/gitea) - Dart SDK for gitea `MIT` `Dart`
- [gitea.js](https://github.com/waspothegreat/gitea.js) - Gitea (WIP) wrapper lib made in javascript. `AGPL-3.0` `JavaScript`
- [Gitea.net](https://github.com/mkloubert/gitea.net) - .NET Library for the Gitea API. `MIT` `C#`
- [Giteapy](https://pypi.org/project/giteapy/) - Python SDK for gitea `N/A` `Python`
- [gitear](https://CRAN.R-project.org/package=gitear) - R wrapper to the gitea API `GPL-3.0` `R`
- [Gitea rust crate](https://crates.io/crates/gitea) - A simple Gitea client for Rust programs `MIT` `Rust`
- [java-gitea-api](https://github.com/zeripath/java-gitea-api) - Swagger generated api for Gitea `MIT` `Java`
- [PHP](https://github.com/avency/Gitea/) - PHP SDK for gitea `MIT` `PHP`
- [Sugar Cube Client](https://github.com/sitelease/sugar-cube-client) - A sweet Gitea API client for PHP `MIT` `PHP`

### Themes

#### Light

- [Red Silver](https://github.com/iamdoubz/Gitea-Red-Silver) - Red silver theme by iamdoubz `MIT` `CSS`
- [lstolcman/GitHub](https://github.com/lstolcman/gitea-github-theme) - Simple Github theme for Gitea `N/A` `CSS`

#### Dark

- [Carbon Red](https://github.com/iamdoubz/Gitea-Carbon-Red) - Darker red 1.14+ theme based on arc-green by iamdoubz `MIT` `CSS`
- [Dark Red](https://github.com/iamdoubz/Gitea-Dark-Red-Theme) - Dark red theme by iamdoubz `MIT` `CSS`
- [Matrix](https://github.com/TylerByte666/gitea-matrix-template) - Neon-green with a matrix-inspired background `MIT` `CSS`
- [Pitch Black](https://github.com/iamdoubz/Gitea-Pitch-Black) - Pitch black 1.14+ theme used on [https://git.dou.bet/iamdoubz/Gitea-Pitch-Black](https://git.dou.bet/iamdoubz/Gitea-Pitch-Black) `MIT` `CSS`

## Archived

### Applications

#### Bot

- [tea-cloc](https://codeberg.org/qwerty287/tea-cloc) - Bot to count lines of code on Gitea repos and comments on pull requests with code change statistics. `MIT` `Go`

#### Panel

- [Listea](https://github.com/IGLOU-EU/listea) - Simple Gitea issues viewer from the tray. `MIT` `Go`

### Packages

- [gitea-helm-chart](https://github.com/jfelten/gitea-helm-chart) - Third-party Helm chart (repository archived on GitHub; use the [official chart](https://gitea.com/gitea/helm-chart) instead). `MIT` `Helm`
- [gitea-operator](https://github.com/integr8ly/gitea-operator) - An Operator that installs Gitea `Apache-2.0` `Go`

### SDK

- [Gitea-sdk](https://gitea.com/jolheiser/gitea-sdk) - Gitea SDK generated by Swagger. (Archived, use the official Golang SDK) `N/A` `Go`

### Themes

- [Modern](https://codeberg.org/Freeplay/Gitea-Modern) - Changes the layout for a more modern look. Usable with other themes that only change colors. `GPL-3.0` `CSS`
- [Red](https://github.com/saegl5/Gitea-Red) - Red theme by saegl5 (forked from Red Silver) `MIT` `CSS`

### Workflow Tools

- [alfred-gitea](https://github.com/pat-s/alfred-gitea) - Alfred workflow for Gitea (repository archived on GitHub). `MIT` `Python`

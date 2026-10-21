# Putting mobiler.rs live

The one-time setup that publishes this site at `https://mobiler.rs` through GitHub Pages. After it,
deploys are automatic (see [Updates](#6-updates)).

Do the steps in order. Only step 3 happens outside GitHub.

## 1. Turn on Pages, deployed by GitHub Actions

1. Open **github.com/mobiler/mobiler → Settings → Pages**.
2. Under **Build and deployment → Source**, choose **GitHub Actions** (not "Deploy from a branch").

This also creates the `github-pages` environment the workflow deploys to. By default only `main` may
deploy to it.

## 2. Set the custom domain in GitHub

1. On the same page, under **Custom domain**, enter `mobiler.rs` and click **Save**.
2. GitHub then checks DNS. That check fails until step 3 is done; that is expected.

The domain must be set here: a deploy from GitHub Actions ignores a `CNAME` file, so the repository
has none.

**Recommended: verify the domain,** so no one else can point their own Pages site at it.

1. Open **github.com/organizations/mobiler/settings/pages** → **Add a domain** → `mobiler.rs`.
2. GitHub shows a TXT record (name `_github-pages-challenge-mobiler.mobiler.rs`, plus a value).
3. Add that TXT record at your DNS provider, then click **Verify**.

## 3. DNS, at the `.rs` registrar or DNS provider

`mobiler.rs` is an apex domain (nothing in front of it), so it needs A and AAAA records, not a CNAME:

| Type  | Name  | Value                 |
|-------|-------|-----------------------|
| A     | `@`   | `185.199.108.153`     |
| A     | `@`   | `185.199.109.153`     |
| A     | `@`   | `185.199.110.153`     |
| A     | `@`   | `185.199.111.153`     |
| AAAA  | `@`   | `2606:50c0:8000::153` |
| AAAA  | `@`   | `2606:50c0:8001::153` |
| AAAA  | `@`   | `2606:50c0:8002::153` |
| AAAA  | `@`   | `2606:50c0:8003::153` |
| CNAME | `www` | `mobiler.github.io`   |

- Delete any other A, AAAA or URL-forwarding record on `@` (a parking page, for example), or they
  clash with these.
- With the `www` record, GitHub redirects `www.mobiler.rs` to `mobiler.rs`.
- Don't add a wildcard record (`*.mobiler.rs`).

Check it (changes take from minutes to a few hours):

```sh
dig +short mobiler.rs A          # the four 185.199.x.153 addresses
dig +short mobiler.rs AAAA       # the four 2606:50c0:800x::153 addresses
dig +short www.mobiler.rs CNAME  # mobiler.github.io.
```

## 4. First deploy

```sh
gh workflow run site-deploy.yml --ref main
gh run list --workflow=site-deploy.yml -L1    # its status
```

You can also go to **Actions → Deploy site → Run workflow** on `main`. The workflow has two jobs:

- `build`: `npm ci`, then generate and build the site, checking links and quoted versions;
- `deploy`: publish the built site to Pages.

It takes about two minutes.

Ignore the default address, `mobiler.github.io/mobiler/`. The site is built for the root of a
domain, so on that sub-path its styles and links break. Judge it at `https://mobiler.rs` only.

## 5. HTTPS

1. Once DNS resolves, the custom-domain check on **Settings → Pages** turns green.
2. GitHub then requests a Let's Encrypt certificate, which takes up to about an hour.
3. When the certificate is ready, tick **Enforce HTTPS**.

## 6. Updates

The site redeploys by itself on every push to `main` that changes what it is built from:

- `site/`
- the Saldo tutorial (`docs/tutorial/saldo/`)
- `capabilities.json`
- a plugin manifest (`mobiler/plugins/*/mobiler-plugin.toml`)
- a demo screenshot (`demos/*/screenshots/`)
- the deploy workflow itself

To deploy at any other time, run the workflow by hand (step 4).

The `site` job in CI builds every pull request the same way, so a page that breaks the build is caught
before it reaches `main`.

## If something goes wrong

- **The workflow fails in `deploy` with an environment or branch error:** check that step 1 chose
  GitHub Actions. Also check that **Settings → Environments → github-pages** allows `main`.
- **The custom domain check stays red:** run the `dig` commands above. Every record must match and
  nothing else may sit on `@`.
- **The page shows unstyled HTML:** you are on `mobiler.github.io/mobiler/`. Use `https://mobiler.rs`.
- **"Enforce HTTPS" is greyed out:** the certificate isn't issued yet. Wait, or remove the custom domain
  and save it again to retry.

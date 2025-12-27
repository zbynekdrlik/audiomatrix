# Branch Protection Rules Setup

Configure these rules in GitHub repository Settings → Branches after pushing.

## `main` Branch Protection

### Required Settings

1. **Require a pull request before merging**
   - [x] Require approvals: 1
   - [x] Dismiss stale pull request approvals when new commits are pushed
   - [x] Require approval of the most recent reviewable push

2. **Require status checks to pass before merging**
   - [x] Require branches to be up to date before merging
   - Required checks:
     - `CI Complete`
     - `Format Check`
     - `Clippy Lints`
     - `Test (ubuntu-latest)`
     - `Test (windows-latest)`
     - `Test (macos-latest)`
     - `Code Coverage`
     - `Security Audit`
     - `Documentation`
     - `File Size Check`

3. **Require conversation resolution before merging**
   - [x] Enabled

4. **Require signed commits**
   - [ ] Optional (enable if team uses GPG signing)

5. **Require linear history**
   - [x] Enabled (squash or rebase only)

6. **Do not allow bypassing the above settings**
   - [x] Enabled (even admins must follow rules)

7. **Restrict who can push to matching branches**
   - [x] Enabled (only via PR)

## `develop` Branch Protection

### Required Settings

1. **Require a pull request before merging**
   - [x] Require approvals: 1

2. **Require status checks to pass before merging**
   - Required checks:
     - `Format Check`
     - `Clippy Lints`
     - `Test (ubuntu-latest)`

3. **Allow force pushes**
   - [ ] Disabled

## Setup via GitHub CLI

```bash
# After creating the repository, run:

# Protect main branch
gh api repos/{owner}/{repo}/branches/main/protection -X PUT \
  -F required_status_checks='{"strict":true,"contexts":["CI Complete"]}' \
  -F enforce_admins=true \
  -F required_pull_request_reviews='{"required_approving_review_count":1,"dismiss_stale_reviews":true}' \
  -F restrictions=null \
  -F required_linear_history=true \
  -F allow_force_pushes=false \
  -F allow_deletions=false
```

## Rulesets (Alternative - GitHub Rulesets)

For more granular control, use GitHub Rulesets (Settings → Rules → Rulesets).

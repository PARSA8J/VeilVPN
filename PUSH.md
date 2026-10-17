# Push this repo to your GitHub — 2 ways

## Way 1 — with git (recommended, 4 commands)

1. Create an EMPTY repo on github.com/new (name: `VeilVPN`, no README/license)
2. Then run inside this folder:

```bash
git remote add origin https://github.com/<YOUR-USERNAME>/VeilVPN.git
git branch -M main
git push -u origin main
```

3. Done. GitHub Actions will build the APK + EXE automatically (Actions tab),
   and pushing a tag `v1.5.2` etc. creates a Release with both files attached.

## Way 2 — web upload (no git)

1. Unzip this folder somewhere
2. github.com/new → create empty repo `VeilVPN`
3. On the repo page click **"uploading an existing file"**
4. Drag ALL files/folders from the unzipped folder (including `.github/`) into the page
5. Commit — Actions will build automatically

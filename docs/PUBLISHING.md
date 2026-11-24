# Milestone publication

The public destination is https://github.com/mkr2tl/xenstride-public.
It starts with a reviewed source export and a new Git root. The original private
repository and its branches, tags and pull requests remain private and unchanged.
Local source commit IDs in provenance identify origins; their Git objects are not imported.

Keep daily development local. Publish only completed, reviewed milestones.

1. Select an immutable source commit and its validation report; never export a live worktree.
2. Copy only approved files. Keep MIT tools and GPL contributions separate, retaining
   licences, exact provenance, tests and known failures. Do not merge development history.
3. Review the diff against the last public milestone, including dependency/licence impact.
   Refresh both manifests, then run `python -B scripts/check_publication.py --require-history`
   in a full checkout and run the lightweight tests from `tools-mit`.
4. Commit locally with a short factual message. Use a GitHub noreply address for both author
   and committer; inspect their values in every new commit. AI-assisted changes remain disclosed.
5. Obtain owner approval before creating a remote repository, pushing, changing visibility
   or submitting tracker applications. Show the exact local commit and check results first.
6. After approval, publish only this repository to the new destination. Never push refs or
   objects from the old private repository. Review the new remote refs and GitHub-generated
   PR metadata before any later visibility change. Keep GitHub email privacy enabled and
   verify the account uses its noreply address for web-created commits and merges.

Review recordings separately. Do not add raw logs, dumps, game material or unapproved links.
Do not sync recursively from GTA6-Lab or publish every development commit.
The root GitHub Actions workflow checks the full history and component manifests.

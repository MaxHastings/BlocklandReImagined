#!/usr/bin/env python3
"""Record GitHub's latest public release on main through its required CI check."""

import argparse
import os
from pathlib import Path
import re
import subprocess
import time
from urllib.parse import quote

from publish_release import gh, gh_json, repository


STATUS = Path('docs/STATUS.md')
START = '<!-- latest-release:start -->\n'
END = '<!-- latest-release:end -->'
STAGING = 'automation/release-record-'


def release_record(repo):
    release = gh_json('api', f'repos/{repo}/releases/latest')
    version = release['tag_name']
    date = release['published_at']
    if (release['draft'] or release['prerelease']
            or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,63}', version)
            or not re.fullmatch(r'\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ', date or '')):
        raise ValueError('GitHub returned invalid latest release metadata')
    tag = quote(version, safe='')
    sha = gh_json('api', f'repos/{repo}/commits/{tag}')['sha']
    if not re.fullmatch(r'[0-9a-f]{40}', sha):
        raise ValueError(f'GitHub returned an invalid commit for {version}')
    url = f'https://github.com/{repo}'
    lines = [
        f'Published release [{version}]({url}/releases/tag/{tag})',
        f'on {date} from [`{sha}`]({url}/commit/{sha}).',
    ]
    if any(asset['name'] == 'SHA256SUMS' for asset in release['assets']):
        lines.append(f'[Release checksums]({url}/releases/download/{tag}/SHA256SUMS).')
    return version, '\n'.join(lines) + '\n'


def replace_record(status, record):
    if status.count(START) != 1 or status.count(END) != 1:
        raise ValueError('Expected one latest-release block in docs/STATUS.md')
    before, remaining = status.split(START)
    if END not in remaining:
        raise ValueError('Latest-release markers are in the wrong order')
    _, after = remaining.split(END)
    return before + START + record + END + after


def git(*args):
    return subprocess.check_output(['git', *args], text=True).strip()


def validate_stage(branch):
    if not re.fullmatch(re.escape(STAGING) + r'\d+-\d+-\d+', branch):
        raise ValueError('Release-record CI must run on a release record staging branch')
    git('fetch', 'origin', 'main')
    if (git('rev-list', '--count', 'origin/main..HEAD') != '1'
            or len(git('rev-list', '--parents', '-n', '1', 'HEAD').split()) != 2
            or git('diff', '--name-only', 'origin/main...HEAD') != STATUS.as_posix()):
        raise ValueError('A release record commit may only update docs/STATUS.md')
    before = subprocess.check_output(['git', 'show', f'HEAD^:{STATUS.as_posix()}'], text=True)
    after = STATUS.read_text(encoding='utf-8')
    record = after.split(START, 1)[-1].split(END, 1)[0]
    if after != replace_record(before, record):
        raise ValueError('A release record commit may only change the generated block')


def check_commit(repo, branch, sha):
    gh('workflow', 'run', 'ci.yml', '--repo', repo, '--ref', branch, '-f', 'release_record=true')
    endpoint = f'repos/{repo}/actions/workflows/ci.yml/runs?head_sha={sha}&event=workflow_dispatch'
    for _ in range(20):
        for run in gh_json('api', endpoint)['workflow_runs']:
            if run['head_branch'] == branch and run['head_sha'] == sha:
                result = subprocess.run(
                    ['gh', 'run', 'watch', str(run['id']), '--repo', repo,
                     '--exit-status', '--interval', '10'], timeout=900,
                )
                if result.returncode:
                    return None
                jobs = gh_json('api', f"repos/{repo}/actions/runs/{run['id']}/jobs")['jobs']
                return next((job['html_url'] for job in jobs
                             if job['name'] == 'check' and job['head_sha'] == sha
                             and job['conclusion'] == 'success'), None)
        time.sleep(3)
    raise RuntimeError(f'CI run did not appear for {branch} at {sha}')


def sync(repo):
    run_id = os.environ['GITHUB_RUN_ID']
    run_attempt = os.environ['GITHUB_RUN_ATTEMPT']
    if git('status', '--porcelain'):
        raise ValueError('Release record sync needs a clean workflow checkout')
    for attempt in range(1, 4):
        git('fetch', 'origin', 'main')
        base = git('rev-parse', 'origin/main')
        git('checkout', '--detach', base)
        version, record = release_record(repo)
        previous = STATUS.read_text(encoding='utf-8')
        updated = replace_record(previous, record)
        if previous == updated:
            print(f'main already records {version}')
            return

        STATUS.write_text(updated, encoding='utf-8', newline='\n')
        git('add', '--', STATUS.as_posix())
        git('commit', '-m', f'Record published release {version}')
        sha = git('rev-parse', 'HEAD')
        branch = f'{STAGING}{run_id}-{run_attempt}-{attempt}'
        print(f'Checking release record {sha} for {version}.', flush=True)
        git('push', 'origin', f'HEAD:refs/heads/{branch}')
        try:
            check_url = check_commit(repo, branch, sha)
            if release_record(repo) != (version, record):
                print('Latest release changed during CI. Refreshing the record.')
                continue
            if not check_url:
                raise RuntimeError(f'CI failed for release record commit {sha}')
            # Branch protection does not count the job from a workflow_dispatch
            # run. Attach its verified result directly to the same commit.
            gh('api', '--method', 'POST', f'repos/{repo}/statuses/{sha}',
               '-f', 'context=check', '-f', 'state=success',
               '-f', 'description=Release record CI passed', '-f', f'target_url={check_url}')
            if subprocess.run(['git', 'push', 'origin', 'HEAD:refs/heads/main']).returncode == 0:
                print(f'main now records {version} at {sha}')
                return
            git('fetch', 'origin', 'main')
            if git('rev-parse', 'origin/main') == base:
                raise RuntimeError('GitHub rejected the checked release record commit')
            print('main advanced during CI. Rebuilding the record from the new tip.')
        finally:
            subprocess.run(['git', 'push', 'origin', '--delete', branch])
    raise RuntimeError('main or the latest release kept changing during record sync')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='compare local status with GitHub without writing')
    parser.add_argument('--validate-stage', metavar='BRANCH', help='verify the restricted CI input')
    args = parser.parse_args()
    if args.validate_stage:
        validate_stage(args.validate_stage)
    elif args.check:
        version, record = release_record(repository())
        status = STATUS.read_text(encoding='utf-8')
        if status != replace_record(status, record):
            raise SystemExit(f'{STATUS} needs to be updated to {version}')
        print(f'{STATUS} records {version}')
    else:
        sync(repository())


if __name__ == '__main__':
    main()

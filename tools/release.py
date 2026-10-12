#!/usr/bin/env python3
"""One command from a gate-ready branch to a published release, on the PC.

    python tools/release.py v0.2.11 --branch claude/v0211-integration-ab12cd
    python tools/release.py v0.2.11 --branch <branch> --check-only   gate and build, publish nothing

It does, in this order, and stops at the first failure with what failed:

  1. Refresh content for the release commit: the main checkout checks out the
     branch's head (detached), `tools/bootstrap.py` rebuilds only the packs
     whose importer inputs changed, then `tools/ci_content.py upload` when a
     pack changed and `tools/addon_bundle.py upload` when the content, the
     Add-On list, the importer or a port changed since the previous v* tag.
     The gate then tests the content that ships.
  2. Start the build-only release.yml run at the branch (Windows, macOS and
     Linux side by side on GitHub), which runs while the gate does.
  3. `tools/gate.py` on the release commit.
  4. Push that exact commit to main (the push hook reuses the gate's pass),
     tag it and push the tag. publish-release.yml then publishes the zips
     step 2 built, with docs/releases/<version>.md as the notes when present.
  5. Put the main checkout back on main, wait for the publish run and print
     the release link.

Before it starts: the branch must contain origin/main and the main checkout
must have no tracked changes. Each step prints its minutes, so the next
release's timeline is in the log. See docs/release-builds.md.
"""
import argparse
import json
import pathlib
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import gate  # noqa: E402

# A change under these paths since the previous release changes what the
# bundled originals import to, so the Add-On bundle is rebuilt and uploaded.
BUNDLE_INPUTS = ('packages/default-addons.json', 'crates/addon-import/')
STARTED = time.time()


class ReleaseError(Exception):
    pass


def say(message):
    print(f'[release {(time.time() - STARTED) / 60:5.1f} min] {message}', flush=True)


def run(*command, cwd=None):
    say('$ ' + ' '.join(str(part) for part in command))
    if subprocess.run([str(part) for part in command], cwd=cwd).returncode != 0:
        raise ReleaseError(f"{' '.join(str(part) for part in command)} failed")


def output(*command, cwd=None):
    result = subprocess.run([str(part) for part in command], cwd=cwd, capture_output=True, text=True,
                            encoding='utf-8', errors='replace')
    if result.returncode != 0:
        raise ReleaseError(f"{' '.join(str(part) for part in command)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def bundle_inputs_changed(changed):
    return any(path.startswith(BUNDLE_INPUTS) for path in changed)


def previous_release(sha, checkout):
    result = subprocess.run(['git', 'describe', '--tags', '--abbrev=0', '--match', 'v[0-9]*', sha],
                            cwd=checkout, capture_output=True, text=True)
    return result.stdout.strip() or None


def preflight(version, branch, checkout):
    output('gh', 'auth', 'status')
    run('git', 'fetch', '-q', 'origin', 'main', branch, '--tags', cwd=checkout)
    if subprocess.run(['git', 'rev-parse', '-q', '--verify', f'refs/tags/{version}'], cwd=checkout,
                      capture_output=True).returncode == 0:
        raise ReleaseError(f'tag {version} already exists; pick a new version')
    sha = output('git', 'rev-parse', f'refs/remotes/origin/{branch}', cwd=checkout)
    if subprocess.run(['git', 'merge-base', '--is-ancestor', 'refs/remotes/origin/main', sha],
                      cwd=checkout).returncode != 0:
        raise ReleaseError(f'origin/{branch} ({sha[:9]}) does not contain origin/main; merge main into it first')
    if output('git', 'status', '--porcelain', '--untracked-files=no', cwd=checkout):
        raise ReleaseError(f'{checkout} has uncommitted changes; the content refresh checks out the release commit there')
    return sha


def refresh_content(sha, checkout, force_upload):
    content = checkout / 'content'
    before = gate.content_fingerprint(content)
    run('git', 'checkout', '-q', '--detach', sha, cwd=checkout)
    run(sys.executable, checkout / 'tools' / 'bootstrap.py', cwd=checkout)
    packs_changed = gate.content_fingerprint(content) != before
    if packs_changed or force_upload:
        run(sys.executable, checkout / 'tools' / 'ci_content.py', 'upload', cwd=checkout)
    else:
        say('no content pack changed; the uploaded content stays')
    previous = previous_release(sha, checkout)
    changed = output('git', 'diff', '--name-only', previous, sha, cwd=checkout).split() if previous else []
    if packs_changed or force_upload or previous is None or bundle_inputs_changed(changed):
        run(sys.executable, checkout / 'tools' / 'addon_bundle.py', 'upload', cwd=checkout)
    else:
        say(f'no Add-On list, importer, port or content change since {previous}; the uploaded bundle stays')


def start_build(version, branch, sha, checkout):
    """Dispatch the build-only release.yml run and return its id."""
    run('gh', 'workflow', 'run', 'release.yml', '--ref', branch, '-f', f'version={version}', '-f', 'publish=false',
        cwd=checkout)
    for _ in range(30):
        time.sleep(5)
        runs = json.loads(output('gh', 'run', 'list', '--workflow', 'release.yml', '--branch', branch,
                                 '--event', 'workflow_dispatch', '--limit', '5',
                                 '--json', 'databaseId,headSha,url', cwd=checkout))
        for entry in runs:
            if entry['headSha'] == sha:
                say(f"release builds running: {entry['url']}")
                return entry['databaseId']
    raise ReleaseError('the release.yml run did not appear; check the Actions tab')


def build_finished_ok(run_id, checkout):
    result = subprocess.run(['gh', 'run', 'view', str(run_id), '--json', 'status,conclusion'], cwd=checkout,
                            capture_output=True, text=True)
    if result.returncode:
        return None
    data = json.loads(result.stdout)
    return None if data['status'] != 'completed' else data['conclusion'] == 'success'


def publish(version, sha, checkout):
    run('git', 'push', 'origin', f'{sha}:refs/heads/main', cwd=checkout)
    run('git', 'tag', version, sha, cwd=checkout)
    run('git', 'push', 'origin', f'refs/tags/{version}', cwd=checkout)


def watch_publish(version, checkout):
    for _ in range(30):
        time.sleep(5)
        runs = json.loads(output('gh', 'run', 'list', '--workflow', 'publish-release.yml', '--limit', '5',
                                 '--json', 'databaseId,headBranch,url', cwd=checkout))
        mine = [entry for entry in runs if entry['headBranch'] == version]
        if mine:
            say(f"publishing: {mine[0]['url']}")
            run('gh', 'run', 'watch', str(mine[0]['databaseId']), '--exit-status', '--interval', '30', cwd=checkout)
            return
    raise ReleaseError('the publish-release run did not appear; check the Actions tab')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('version', help='the release version and tag, such as v0.2.11')
    parser.add_argument('--branch', required=True, help='the gate-ready branch on origin, whose head is released')
    parser.add_argument('--check-only', action='store_true',
                        help='refresh content, build and gate, but push, tag and publish nothing')
    parser.add_argument('--skip-refresh', action='store_true',
                        help='leave content/ and the uploads as they are (they already match this commit)')
    parser.add_argument('--upload', action='store_true', help='upload content and the Add-On bundle even if unchanged')
    args = parser.parse_args()
    checkout = gate.main_checkout()
    phases = []

    def phase(name, action):
        started = time.time()
        result = action()
        phases.append((name, time.time() - started))
        say(f'{name}: done in {(time.time() - started) / 60:.1f} min')
        return result

    try:
        sha = phase('preflight', lambda: preflight(args.version, args.branch, checkout))
        say(f'releasing {args.version} from origin/{args.branch} at {sha[:9]}')
        if not args.skip_refresh:
            phase('content refresh', lambda: refresh_content(sha, checkout, args.upload))
        build = phase('start release builds', lambda: start_build(args.version, args.branch, sha, checkout))
        passed = phase('gate', lambda: subprocess.run([sys.executable, checkout / 'tools' / 'gate.py', sha],
                                                      cwd=checkout).returncode == 0)
        if not passed:
            raise ReleaseError(f'the gate failed on {sha[:9]}; the log path is printed above. Nothing was pushed.')
        if build_finished_ok(build, checkout) is False:
            raise ReleaseError(f'release build run {build} failed; nothing was pushed (gh run view {build} --log-failed)')
        if args.check_only:
            say('check only: gate passed; nothing pushed, tagged or published')
            return
        phase('push main and tag', lambda: publish(args.version, sha, checkout))
        phase('back to main', lambda: (run('git', 'checkout', '-q', 'main', cwd=checkout),
                                       run('git', 'merge', '-q', '--ff-only', 'origin/main', cwd=checkout)))
        phase('publish', lambda: watch_publish(args.version, checkout))
        say('release: ' + output('gh', 'release', 'view', args.version, '--json', 'url', '-q', '.url', cwd=checkout))
    except (ReleaseError, gate.GateError) as error:
        say(f'STOPPED: {error}')
        if output('git', 'rev-parse', '--abbrev-ref', 'HEAD', cwd=checkout) == 'HEAD':
            say(f'{checkout} is left on the release commit with its refreshed content; '
                'rerun this command after the fix, or `git checkout main` there.')
        sys.exit(1)
    finally:
        for name, seconds in phases:
            print(f'  {name:<22} {seconds / 60:5.1f} min')
        print(f'  {"total":<22} {(time.time() - STARTED) / 60:5.1f} min')


if __name__ == '__main__':
    main()

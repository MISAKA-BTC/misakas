"""Hash candidate source/test inputs; exclude build outputs and mutable evidence logs."""
import hashlib, json, pathlib, subprocess, sys
ROOT = pathlib.Path(__file__).resolve().parents[4]
def manifest():
    paths = set(subprocess.check_output(['git','ls-files','-co','--exclude-standard','-z'], cwd=ROOT).decode().split('\0')) - {''}
    files = {}
    for name in sorted(paths):
        first = name.split('/')[0]
        if first.startswith('target') or (first.startswith('.') and first != '.cargo'):
            continue
        if first == 'docs' and not name.startswith('docs/design/'):
            continue
        if name.endswith(('.log','.sample','.tmp','.pyc')) or name.startswith('scripts/release/arming-runs-17400/'):
            continue
        path = ROOT/name
        if not path.is_file():
            continue
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            for chunk in iter(lambda: stream.read(1024*1024), b''):
                digest.update(chunk)
        files[name] = digest.hexdigest()
    return {'scope':'repository source/test inputs and docs/design fixtures; excludes build outputs, hidden release directories, logs, and mutable release/evidence output', 'files':files, 'aggregate_sha256':hashlib.sha256(json.dumps(files,sort_keys=True,separators=(',',':')).encode()).hexdigest()}
if __name__ == '__main__':
    result = manifest()
    pathlib.Path(sys.argv[1]).write_text(json.dumps(result,indent=2)+'\n')
    print(result['aggregate_sha256'],len(result['files']))

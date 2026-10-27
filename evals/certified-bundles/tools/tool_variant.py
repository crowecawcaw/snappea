"""Matched CLI treatments; Python observations always use the native protocol."""
import hashlib
from pathlib import Path

TOOLS = ('snappea', 'snappea-compat')

def cli_binary(native, tool, override=None):
    if tool not in TOOLS:
        raise ValueError(f'unknown tool: {tool}')
    return Path(override).resolve() if override else Path(native).resolve().with_name(tool)

def mounts(native, tool, override=None):
    cli = cli_binary(native, tool, override)
    return ['--mount', f'type=bind,src={cli},dst=/opt/{tool},readonly',
            '--mount', f'type=bind,src={Path(native).resolve()},dst=/opt/python-snappea,readonly',
            '--env', 'SNAPPEA_BIN=/opt/python-snappea']

def instruction(tool):
    return (f'Use {tool} for P model checks (available at /opt/{tool}).\n'
            'The Python library uses /opt/python-snappea internally; do not invoke that backend directly.\n')

def metadata(native, tool, override=None):
    cli = cli_binary(native, tool, override)
    return {'tool': tool, 'cli_binary': str(cli),
            'cli_binary_sha256': hashlib.sha256(cli.read_bytes()).hexdigest(),
            'python_backend_binary': str(Path(native).resolve()),
            'python_backend_sha256': hashlib.sha256(Path(native).read_bytes()).hexdigest()}


def verify(meta):
    """Reject changed selected CLI or Python backend before freeze/regrading."""
    for path_key, hash_key in (('cli_binary', 'cli_binary_sha256'),
                               ('python_backend_binary', 'python_backend_sha256')):
        if path_key in meta:
            path = Path(meta[path_key])
            if hashlib.sha256(path.read_bytes()).hexdigest() != meta[hash_key]:
                raise ValueError(f'frozen tool binary changed: {path_key}')

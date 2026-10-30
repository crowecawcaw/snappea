import shutil
import subprocess
from pathlib import Path


def test_p_model():
    here = Path(__file__).parent
    out = subprocess.run(["snappea", "check", "Names.p", "--no-mutation", "--timeout", "30s"],
                         cwd=here, capture_output=True, text=True)
    assert '"result": "ok"' in out.stdout and '"search_complete": true' in out.stdout, out.stdout

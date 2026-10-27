"""Global mode: the pytest plugin, run on small throwaway projects."""

import shutil

import pytest
from conftest import HERE, SPEC

APP = f"{HERE}/example/app/locks.py"

TESTS = '''
import pytest
from locks import LockServer
from snappea import emit, expect_violation


class Clock:
    t = 0.0
    def __call__(self):
        return self.t


def test_correct():
    clock = Clock()
    server = LockServer(lease=1, clock=clock)
    server.acquire("c1")
    clock.t = 2
    server.acquire("c2")


def test_buggy_expiry():
    clock = Clock()
    server = LockServer(lease=1, clock=clock, buggy_expiry=True)
    server.acquire("c1")
    clock.t = 2
    server.acquire("c2")


def test_no_events():
    assert 1 + 1 == 2


@pytest.mark.snappea(skip=True)
def test_skipped_on_purpose():
    emit("eEnter", client="a")
    emit("eEnter", client="b")


@pytest.mark.snappea(skip=["MutualExclusion"])
def test_skip_one_spec():
    emit("eEnter", client="a")
    emit("eEnter", client="b")


def test_waiting_forever_passes_without_check_hot():
    LockServer().acquire("c1")
    LockServer.acquire(LockServer(), "c2", "L2")
    emit("eRequest", key="L3", client="c3")


@pytest.mark.snappea(check_hot=True)
def test_waiting_forever_fails_with_check_hot():
    emit("eRequest", key="L3", client="c3")


def test_fixture(snappea_events):
    server = LockServer()
    server.acquire("c1")
    server.release("c1")
    assert snappea_events.count("eEnter") == 1


def test_expected_violation_is_not_reported_twice():
    with expect_violation("MutualExclusion"):
        emit("eEnter", client="a")
        emit("eEnter", client="b")


def test_failing_test_is_not_also_checked():
    emit("eEnter", client="a")
    emit("eEnter", client="b")
    assert False, "the test's own failure"
'''


@pytest.fixture
def project(pytester):
    pytester.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\n')
    (pytester.path / "specs").mkdir()
    shutil.copy(SPEC, pytester.path / "specs" / "lock.p")
    shutil.copy(APP, pytester.path / "locks.py")
    pytester.makepyfile(test_locks=TESTS)
    return pytester


def run(pytester, *args):
    return pytester.runpytest_subprocess("-p", "no:cacheprovider", *args)


def test_every_test_is_checked(project):
    r = run(project, "-v")
    r.assert_outcomes(passed=7, failed=3)
    r.stdout.fnmatch_lines(
        [
            "snappea: checking P specs from specs/*.p (auto)",
            "*test_buggy_expiry FAILED*",
            "*test_waiting_forever_passes_without_check_hot PASSED*",
            "*test_waiting_forever_fails_with_check_hot FAILED*",
            "*test_expected_violation_is_not_reported_twice PASSED*",
        ]
    )
    out = r.stdout.str()
    assert "P spec MutualExclusion was broken: c2 entered while c1 is inside" in out
    assert "locks.py:27  <- broke the spec here" in out
    assert "Spec: specs/lock.p:19" in out
    assert "P spec RequestsGranted was still waiting when the events ended" in out
    assert "AssertionError: the test's own failure" in out
    # the plugin's frames are hidden: the report points at the test
    assert "pytest_plugin.py" not in out


def test_marked_mode(project):
    r = run(project, "--snappea", "marked")
    # only the marked tests are checked; test_buggy_expiry passes unchecked
    r.assert_outcomes(passed=8, failed=2)


def test_off(project):
    r = run(project, "--snappea", "off")
    r.assert_outcomes(passed=9, failed=1)
    assert "snappea:" not in r.stdout.str()


def test_exclude_setting(project):
    project.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nexclude = ["MutualExclusion"]\n')
    r = run(project)
    r.assert_outcomes(passed=8, failed=2)


def test_without_settings_the_plugin_is_inert(pytester):
    pytester.makepyfile(
        """
        from snappea import emit
        def test_a():
            emit("eEnter", client="a")
        """
    )
    r = run(pytester)
    r.assert_outcomes(passed=1)


def test_bad_settings_are_reported(pytester):
    pytester.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nmode = "sometimes"\n')
    pytester.makepyfile("def test_a(): pass")
    r = run(pytester)
    assert "mode must be one of auto, marked, off" in r.stdout.str() + r.stderr.str()
    assert r.ret != 0


def test_events_in_fixture_setup_count(project):
    project.makepyfile(
        test_fixture_events="""
        import pytest
        from snappea import emit

        @pytest.fixture
        def held_lock():
            emit("eEnter", client="setup")

        def test_double_enter(held_lock):
            emit("eEnter", client="test")
        """
    )
    r = run(project, "test_fixture_events.py")
    r.assert_outcomes(failed=1)
    r.stdout.fnmatch_lines(["*test entered while setup is inside*"])


def test_opt_in_coverage_summary_and_json(project):
    import json
    result = run(project, "-k", "test_correct or test_fixture", "--snappea-coverage=coverage/monitor.json")
    result.assert_outcomes(passed=2, deselected=8)
    result.stdout.fnmatch_lines([
        "*Spec coverage (all completed checks)*",
        "Coverage:",
        "  MutualExclusion: */* statements, */* branches, */* event handlers, 2/2 transition pairs",
        "  RequestsGranted: */* statements, */* branches, */* event handlers, 1/2 condition outcomes, 2/2 transition pairs",
        "Not covered by any observation:",
        "  specs/lock.p:*  RequestsGranted.*",
        "JSON:*monitor.json",
    ])
    report = json.loads((project.path / "coverage" / "monitor.json").read_text())
    assert report["scope"] == "observed_monitor_suite"
    assert report["specs"]
    mutual = next(s for s in report["specs"] if s["spec"] == "MutualExclusion")
    assert mutual["coverage"]["statements"]["covered"] > 0
    assert mutual["coverage"]["handlers"]["covered"] > 0
    assert mutual["coverage"]["transition_pairs"] == {"covered": 2, "total": 2}
    requests = next(s for s in report["specs"] if s["spec"] == "RequestsGranted")
    assert requests["coverage"]["conditions"] == {"covered": 1, "total": 2}
    assert requests["coverage"]["transition_pairs"] == {"covered": 2, "total": 2}
    # P source paths are relative to the project, not to each check's temp folder
    assert all(u["file"] == "specs/lock.p" for s in report["specs"] for u in s["coverage"]["uncovered"])


def test_coverage_union_via_core_cli(project):
    import json
    project.makepyfile(test_overlap="""
        from snappea import emit
        def test_first():
            emit("eEnter", client="a")
            emit("eExit", client="a")
        def test_second():
            emit("eEnter", client="a")
            emit("eExit", client="a")
    """)
    result = run(project, "test_overlap.py", "--snappea-coverage=monitor.json")
    result.assert_outcomes(passed=2)
    report = json.loads((project.path / "monitor.json").read_text())
    mutual = next(s for s in report["specs"] if s["spec"] == "MutualExclusion")
    handlers = mutual["coverage"]["handlers"]
    assert handlers == {"covered": 2, "total": 2}
    assert report["observations"]


def test_failing_observation_keeps_coverage(project):
    import json
    result = run(project, "-k", "test_buggy_expiry", "--snappea-coverage=monitor.json")
    result.assert_outcomes(failed=1, deselected=9)
    report = json.loads((project.path / "monitor.json").read_text())
    mutual = next(s for s in report["specs"] if s["spec"] == "MutualExclusion")
    assert mutual["coverage"]["handlers"]["covered"] > 0
    assert report["observations"]


def test_coverage_reporting_error_fails_process(project):
    # Passing tests must not hide a failed final report write.
    (project.path / "directory.json").mkdir()
    result = run(project, "-k", "test_correct", "--snappea-coverage=directory.json")
    result.assert_outcomes(passed=1, deselected=9)
    assert result.ret != 0
    assert "Spec coverage reporting failed" in result.stdout.str()


def test_coverage_empty_suite(project):
    import json
    result = run(project, "-k", "test_skipped_on_purpose", "--snappea-coverage=empty.json")
    result.assert_outcomes(passed=1, deselected=9)
    report = json.loads((project.path / "empty.json").read_text())
    assert report["specs"] == []


def test_strict_opt_in_checks_no_event_tests(project):
    result = run(project, "-k", "test_no_events", "--snappea-strict")
    result.assert_outcomes(failed=1, deselected=9)
    result.stdout.fnmatch_lines(["*checked no relevant events*"])
    project.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nstrict = true\n')
    result = run(project, "-k", "test_no_events")
    result.assert_outcomes(failed=1, deselected=9)
    result.stdout.fnmatch_lines(["*checked no relevant events*"])


def test_snappea_mode_environment_variable(project, monkeypatch):
    monkeypatch.setenv("SNAPPEA_MODE", "off")
    run(project).assert_outcomes(passed=9, failed=1)
    # --snappea is explicit, so it wins over the environment
    run(project, "--snappea", "marked").assert_outcomes(passed=8, failed=2)


def test_liveness_mark_is_a_deprecated_alias(project):
    project.makepyfile(
        test_old="""
        import pytest
        from snappea import emit

        @pytest.mark.snappea(liveness=True)
        def test_waiting():
            emit("eRequest", key="L3", client="c3")
        """
    )
    r = run(project, "test_old.py")
    r.assert_outcomes(failed=1)
    r.stdout.fnmatch_lines(["*liveness=...) is deprecated; use check_hot=*"])


def test_misspelled_event_fails_the_test(project):
    project.makepyfile(
        test_typo="""
        from snappea import emit

        def test_typo():
            emit("eEnterr", client="a")
        """
    )
    r = run(project, "test_typo.py")
    r.assert_outcomes(failed=1)
    r.stdout.fnmatch_lines(["*eEnterr: 1 time(s), first at test_typo.py:4; did you mean eEnter?*"])
    project.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nunknown_events = "warn"\n')
    r = run(project, "test_typo.py")
    r.assert_outcomes(passed=1, warnings=1)


def test_named_specs_in_settings_do_not_make_plugin_checks_strict(project):
    # the plugin checks every test, and most emit no events for a given spec
    project.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nspecs = ["MutualExclusion"]\n')
    r = run(project, "-k", "test_no_events or test_correct or test_waiting_forever_fails")
    # test_waiting_forever_fails emits only eRequest, which MutualExclusion ignores
    r.assert_outcomes(passed=3, deselected=7)
    r.stdout.no_fnmatch_line("*StrictDefaultWarning*")
    # strict = true opts in, and fails tests whose named spec saw no relevant event
    project.makepyprojecttoml(
        '[tool.snappea]\nspec_files = ["specs/*.p"]\nspecs = ["MutualExclusion"]\nstrict = true\n'
    )
    r = run(project, "-k", "test_no_events or test_correct or test_waiting_forever_fails")
    r.assert_outcomes(passed=1, failed=2, deselected=7)
    r.stdout.fnmatch_lines(["*checked no relevant events*", "*strict=False*"])


def test_strict_mark_opts_one_test_in(project):
    project.makepyfile(test_marked_strict="""
import pytest

@pytest.mark.snappea(strict=True)
def test_marked_no_events():
    pass
""")
    r = run(project, "-k", "test_marked_no_events or test_no_events")
    r.assert_outcomes(passed=1, failed=1, deselected=9)
    r.stdout.fnmatch_lines(["*test_marked_no_events*", "*checked no relevant events*"])


def test_artifacts_setting_keeps_failed_checks(project, monkeypatch):
    project.makepyprojecttoml('[tool.snappea]\nspec_files = ["specs/*.p"]\nartifacts = "snappea-artifacts"\n')
    run(project, "-k", "test_buggy_expiry or test_correct").assert_outcomes(passed=1, failed=1, deselected=8)
    assert len(list((project.path / "snappea-artifacts").iterdir())) == 1
    monkeypatch.setenv("SNAPPEA_ARTIFACTS", str(project.path / "from-env"))
    run(project, "-k", "test_buggy_expiry").assert_outcomes(failed=1, deselected=9)
    assert len(list((project.path / "from-env").iterdir())) == 1


def test_coverage_merges_into_an_existing_file(project, monkeypatch):
    import json

    project.makepyfile(test_split="""
        from snappea import emit
        def test_enter():
            emit("eEnter", client="a")
        def test_exit():
            emit("eEnter", client="a")
            emit("eExit", client="a")
    """)
    monkeypatch.setenv("SNAPPEA_COVERAGE", "cov.json")
    run(project, "test_split.py", "-k", "test_enter").assert_outcomes(passed=1, deselected=1)
    first = json.loads((project.path / "cov.json").read_text())
    run(project, "test_split.py", "-k", "test_exit").assert_outcomes(passed=1, deselected=1)
    report = json.loads((project.path / "cov.json").read_text())
    assert len(report["observations"]) == len(first["observations"]) + 1
    mutual = next(s for s in report["specs"] if s["spec"] == "MutualExclusion")
    assert mutual["coverage"]["handlers"]["covered"] == 2


def test_coverage_with_xdist(project):
    import json

    pytest.importorskip("xdist")
    result = run(project, "-n", "2", "-k", "test_correct or test_fixture or test_skip_one", "--snappea-coverage=cov.json")
    result.assert_outcomes(passed=3)
    report = json.loads((project.path / "cov.json").read_text())
    assert len(report["observations"]) == 3
    result.stdout.fnmatch_lines(["*Spec coverage*", "*JSON:*cov.json*"])

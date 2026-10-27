package snappeatest

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const testSpec = `event eUnit;
event eCount: int;
event ePair: (int, string);
event eNamed: (units: int);
spec Values observes eUnit, eCount, ePair, eNamed {
 start state Ready {
  on eUnit do { }
  on eCount do (n: int) { assert n >= 0, "negative count"; }
  on ePair do (p: (int, string)) { assert p.0 == 7; assert p.1 == "hello"; }
  on eNamed do (p: (units: int)) { assert p.units >= 0; }
 }
}
spec Units observes eUnit { start state Ready { on eUnit do { } } }`

// snappeaBinary is the real checker, or a skip when none is configured or found.
func snappeaBinary(t *testing.T) string {
	t.Helper()
	binary := os.Getenv("SNAPPEA_BIN")
	if binary == "" {
		var err error
		binary, err = exec.LookPath("snappea")
		if err != nil {
			t.Skip("integration: build snappea and set SNAPPEA_BIN to its absolute path")
		}
	}
	return binary
}

func integrationConfig(t *testing.T) Config {
	t.Helper()
	binary := snappeaBinary(t)
	spec := filepath.Join(t.TempDir(), "values.p")
	if err := os.WriteFile(spec, []byte(testSpec), 0o600); err != nil {
		t.Fatal(err)
	}
	return Config{Binary: binary, SpecFiles: []string{spec}, Specs: []string{"Values"}, Artifacts: t.TempDir()}
}

func TestIntegrationPayloadsAndSubsetSpecs(t *testing.T) {
	cfg := integrationConfig(t)
	cfg.Specs = append(cfg.Specs, "Units")
	r := New(t, cfg)
	r.Emit("eUnit", "a", nil)
	r.Emit("eCount", "a", 42)
	r.Emit("ePair", "a", []any{7, "hello"})
	r.Emit("eNamed", "a", struct {
		Units int `json:"units"`
	}{3})
	result, err := r.Verify(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	result.Report[0] = '!'
	cached, err := r.Verify(context.Background())
	if err != nil || !json.Valid(cached.Report) {
		t.Fatalf("corrupted cache: %v", err)
	}
}

func TestIntegrationStrict(t *testing.T) {
	cfg := integrationConfig(t)
	_, err := New(t, cfg).Verify(context.Background())
	setupKind(t, err, "validation")

	subset := cfg
	subset.Specs = []string{"Values", "Units"}
	onlyCounts := New(t, subset)
	onlyCounts.Emit("eCount", "k", 1)
	_, err = onlyCounts.Verify(context.Background())
	if se := setupKind(t, err, "validation"); !strings.Contains(se.Message, "Units") {
		t.Fatalf("validation message: %v", se)
	}

	cfg.Strict = Bool(false)
	if _, err = New(t, cfg).Verify(context.Background()); err != nil {
		t.Fatal(err)
	}
	cfg.Strict, cfg.Specs = nil, nil // every spec: not strict
	r := New(t, cfg)
	r.Emit("eCount", "k", 1)
	if _, err = r.Verify(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestIntegrationViolationFields(t *testing.T) {
	cfg := integrationConfig(t)
	cfg.Specs = nil
	r := New(t, cfg)
	r.Emit("eCount", "key", 1)
	at := lineOf(func() { r.Emit("eCount", "key", -1) })
	_, err := r.Verify(context.Background())
	var v *SpecViolation
	if !errors.As(err, &v) {
		t.Fatalf("want *SpecViolation, got %T: %v", err, err)
	}
	if v.Spec != "Values" || v.Key != "key" || v.Message != "negative count" || !strings.Contains(v.Location, "values.p:") || len(v.Events) != 2 || v.Events[1].Src != at || v.Events[1].Payload != -1.0 {
		t.Fatalf("violation fields: %+v", v)
	}
	if !strings.Contains(v.Error(), at+"  <- broke the spec here") {
		t.Fatalf("failure output:\n%v", v)
	}
	if _, err := os.Stat(v.ArtifactsDir); err != nil {
		t.Fatalf("artifacts: %v", err)
	}
}

func TestIntegrationExpectViolation(t *testing.T) {
	cfg := integrationConfig(t)
	r := Start(t, cfg) // the cleanup check must not report the expected violation again
	r.Emit("eCount", "k", -1)
	v := r.ExpectViolation(t, "Values", "^negative")
	if v.Key != "k" {
		t.Fatalf("key = %q", v.Key)
	}
}

func TestIntegrationUnknownEvents(t *testing.T) {
	cfg := integrationConfig(t)
	r := New(t, cfg)
	r.Emit("eCount", "k", 1)
	r.Emit("eCuont", "k", -1)
	_, err := r.Verify(context.Background())
	if se := setupKind(t, err, "unknown_event"); !strings.Contains(se.Message, "eCuont") {
		t.Fatalf("message: %v", se)
	}
	cfg.UnknownEvents = "ignore"
	r = New(t, cfg)
	r.Emit("eCount", "k", 1)
	r.Emit("eCuont", "k", -1)
	if _, err = r.Verify(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestIntegrationChildProcess(t *testing.T) {
	cfg := integrationConfig(t)
	self, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	r := New(t, cfg)
	r.Emit("eCount", "parent", 1)
	cmd := exec.Command(self)
	cmd.Env = append(r.Env(), "SNAPPEATEST_CHILD=emit")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("child: %v\n%s", err, out)
	}
	files, _ := os.ReadDir(r.EventsDir())
	if len(files) != 1 || !strings.Contains(files[0].Name(), fmt.Sprintf("-%d-", cmd.Process.Pid)) {
		t.Fatalf("child file: %v", files)
	}
	result, err := r.Verify(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Contains(result.Report, []byte(`"events_read": 2`)) {
		t.Fatalf("child events not checked:\n%s", result.Report)
	}
}

func TestIntegrationAddEventsDirAndLogs(t *testing.T) {
	cfg := integrationConfig(t)
	dir := t.TempDir()
	extra := filepath.Join(dir, "volume")
	os.Mkdir(extra, 0o755)
	os.WriteFile(filepath.Join(extra, "svc-1-ab.jsonl"), []byte(`{"event":"eCount","key":"v","timestamp":"2026-01-01T00:00:00Z","payload":2}`+"\n"), 0o600)
	os.WriteFile(filepath.Join(dir, "plain.jsonl"), []byte(`{"event":"eCount","key":"j","timestamp":1,"payload":3}`+"\n"), 0o600)
	os.WriteFile(filepath.Join(dir, "app.log"), []byte("start\ncount 4\ncount -4\n"), 0o600)

	r := New(t, cfg)
	r.AddEventsDir(extra)
	r.AddLog(filepath.Join(dir, "plain.jsonl"), nil)
	r.AddLog(filepath.Join(dir, "app.log"), func(line string) (Event, bool) {
		var n int
		if _, err := fmt.Sscanf(line, "count %d", &n); err != nil {
			return Event{}, false
		}
		return Event{Event: "eCount", Key: "log", Payload: n, Time: time.Unix(100, 0)}, true
	})
	_, err := r.Verify(context.Background())
	var v *SpecViolation
	if !errors.As(err, &v) || v.Key != "log" || v.Events[len(v.Events)-1].Src != "app.log:3" {
		t.Fatalf("want the log's violation with its line as src, got %v", err)
	}
	if !bytes.Contains(r.result.Report, []byte(`"events_read": 4`)) {
		t.Fatalf("inputs not all read:\n%s", r.result.Report)
	}
}

func TestIntegrationSuiteAndFailedCoverage(t *testing.T) {
	cfg := integrationConfig(t)
	suite, err := NewSuite(SuiteConfig{Binary: cfg.Binary})
	if err != nil {
		t.Fatal(err)
	}
	cfg.Suite = suite
	r := New(t, cfg)
	r.Emit("eCount", "key", -1)
	if _, err = r.Verify(context.Background()); err == nil {
		t.Fatal("want a violation")
	}
	if _, err = r.Verify(context.Background()); err == nil {
		t.Fatal("lost cached failure")
	}
	if len(suite.reports) != 1 {
		t.Fatalf("collected %d reports", len(suite.reports))
	}
	path := filepath.Join(t.TempDir(), "monitor.json")
	var human bytes.Buffer
	if err = suite.WriteCoverage(context.Background(), path, &human); err != nil {
		t.Fatal(err)
	}
	data, _ := os.ReadFile(path)
	if !bytes.Contains(data, []byte("negative count")) || !strings.Contains(human.String(), "(some observations failed)") {
		t.Fatalf("missing failed-prefix or human report: %s\n%s", data, human.String())
	}
	if err = suite.WriteCoverage(context.Background(), t.TempDir(), io.Discard); err == nil {
		t.Fatal("hidden coverage write error")
	}
}

// handlersCovered reads the Values spec's handler coverage from a suite file.
func handlersCovered(t *testing.T, path string) int {
	t.Helper()
	var suite struct {
		Scope string `json:"scope"`
		Specs []struct {
			Spec     string
			Coverage struct{ Handlers struct{ Covered int } }
		}
	}
	data, err := os.ReadFile(path)
	if err != nil || json.Unmarshal(data, &suite) != nil || suite.Scope != "observed_monitor_suite" || len(suite.Specs) != 1 {
		t.Fatalf("suite coverage file: %v\n%s", err, data)
	}
	return suite.Specs[0].Coverage.Handlers.Covered
}

var oneEventPerHandler = []struct {
	event   string
	payload any
}{{"eUnit", nil}, {"eCount", 1}, {"ePair", []any{7, "hello"}}, {"eNamed", map[string]int{"units": 1}}}

// Without Main, each check merges into the shared file under a lock.
func TestIntegrationCoveragePerCheck(t *testing.T) {
	cfg := integrationConfig(t)
	cfg.Coverage = filepath.Join(t.TempDir(), "cov", "monitor.json")
	t.Run("group", func(t *testing.T) {
		for _, e := range oneEventPerHandler {
			t.Run(e.event, func(t *testing.T) {
				t.Parallel()
				Start(t, cfg).Emit(e.event, "k", e.payload)
			})
		}
	})
	if got := handlersCovered(t, cfg.Coverage); got != 4 {
		t.Fatalf("handlers covered = %d, want the union of 4 checks", got)
	}
}

// With Main, checks are collected and merged once, with what the file already holds.
func TestIntegrationCoverageWithMain(t *testing.T) {
	cfg := integrationConfig(t)
	cfg.Coverage = filepath.Join(t.TempDir(), "monitor.json")
	check := func(event string, payload any) {
		r := New(t, cfg)
		r.Emit(event, "k", payload)
		if _, err := r.Verify(context.Background()); err != nil {
			t.Fatal(err)
		}
	}
	check("eUnit", nil) // merged into the file now
	collector.mu.Lock()
	collector.inMain = true
	collector.mu.Unlock()
	defer func() {
		collector.mu.Lock()
		collector.inMain = false
		collector.mu.Unlock()
	}()
	check("eCount", 1)
	check("eNamed", map[string]int{"units": 1})
	if got := handlersCovered(t, cfg.Coverage); got != 1 {
		t.Fatalf("merged before Main finished: %d", got)
	}
	var human bytes.Buffer
	if err := flushCoverage(context.Background(), &human, io.Discard); err != nil {
		t.Fatal(err)
	}
	if got := handlersCovered(t, cfg.Coverage); got != 3 || !strings.Contains(human.String(), "3/4 event handlers") {
		t.Fatalf("handlers covered = %d, want 3; summary:\n%s", got, human.String())
	}
}

// inMain simulates Main for one test: checks are collected until flushCoverage.
func inMain(t *testing.T) {
	collector.mu.Lock()
	collector.inMain = true
	collector.mu.Unlock()
	t.Cleanup(func() {
		collector.mu.Lock()
		collector.inMain, collector.paths, collector.implicit = false, nil, nil
		collector.mu.Unlock()
	})
}

const twoStateSpec = `event eGo; event eStay;
spec Flow observes eGo, eStay {
 start state Ready { on eGo goto Done; on eStay do { } }
 state Done { }
 state Never { on eGo do { } }
}`

// With Main and no coverage path, the suite coverage is printed and no file is written.
func TestIntegrationDefaultCoverageReport(t *testing.T) {
	t.Setenv("SNAPPEA_COVERAGE", "")
	run := func(t *testing.T, cfg Config) (string, string) {
		t.Helper()
		inMain(t)
		for _, event := range []string{"eGo", "eGo"} {
			r := New(t, cfg)
			r.Emit(event, "k", nil)
			if _, err := r.Verify(context.Background()); err != nil {
				t.Fatal(err)
			}
		}
		var human, warn bytes.Buffer
		if err := flushCoverage(context.Background(), &human, &warn); err != nil {
			t.Fatal(err)
		}
		return human.String(), warn.String()
	}
	config := func(t *testing.T) Config {
		cfg := integrationConfig(t)
		cfg.WorkDir = t.TempDir()
		if err := os.WriteFile(cfg.SpecFiles[0], []byte(twoStateSpec), 0o600); err != nil {
			t.Fatal(err)
		}
		cfg.Specs = []string{"Flow"}
		return cfg
	}

	t.Run("printed", func(t *testing.T) {
		cfg := config(t)
		human, warn := run(t, cfg)
		for _, want := range []string{"Spec coverage of all checks", "Flow: 2/3 states", "Flow.Ready, on eStay: never handled", "Flow.Never: state never reached"} {
			if !strings.Contains(human, want) {
				t.Errorf("missing %q in:\n%s", want, human)
			}
		}
		if warn != "" {
			t.Errorf("unexpected warning: %s", warn)
		}
		if entries, _ := os.ReadDir(cfg.WorkDir); len(entries) > 0 {
			t.Errorf("default report wrote into %s: %v", cfg.WorkDir, entries)
		}
		entries, _ := os.ReadDir(cfg.Artifacts) // only each check's own folder
		for _, e := range entries {
			if !e.IsDir() || !strings.HasPrefix(e.Name(), "snappeatest-") {
				t.Errorf("default report wrote %s", filepath.Join(cfg.Artifacts, e.Name()))
			}
		}
	})
	t.Run("env-off", func(t *testing.T) {
		t.Setenv("SNAPPEA_COVERAGE", "off")
		cfg := config(t)
		if human, warn := run(t, cfg); human != "" || warn != "" {
			t.Fatalf("SNAPPEA_COVERAGE=off still reported: %q %q", human, warn)
		}
		if _, err := os.Stat(filepath.Join(cfg.WorkDir, "off")); err == nil {
			t.Fatal("off was taken as a path")
		}
	})
	t.Run("config-off", func(t *testing.T) {
		cfg := config(t)
		cfg.Coverage = "off"
		if human, warn := run(t, cfg); human != "" || warn != "" {
			t.Fatalf("Coverage off still reported: %q %q", human, warn)
		}
	})
	t.Run("no-checks", func(t *testing.T) {
		inMain(t)
		var human, warn bytes.Buffer
		if err := flushCoverage(context.Background(), &human, &warn); err != nil || human.Len()+warn.Len() > 0 {
			t.Fatalf("printed without checks: %v %q %q", err, human.String(), warn.String())
		}
	})
	t.Run("error-is-warning", func(t *testing.T) {
		inMain(t)
		s, err := resolve(Config{SpecFiles: []string{"x.p"}, Binary: filepath.Join(t.TempDir(), "missing")})
		if err != nil {
			t.Fatal(err)
		}
		if err = addCoverage(s, []byte(`{}`)); err != nil {
			t.Fatal(err)
		}
		var human, warn bytes.Buffer
		if err = flushCoverage(context.Background(), &human, &warn); err != nil {
			t.Fatalf("default report failed the run: %v", err)
		}
		if human.Len() > 0 || !strings.HasPrefix(warn.String(), "snappeatest: warning: no suite spec coverage report: ") || strings.Count(warn.String(), "\n") != 1 {
			t.Fatalf("want a one-line warning, got %q %q", human.String(), warn.String())
		}
	})
}

func TestIntegrationParserErrorAndHotCompletion(t *testing.T) {
	t.Run("payload-mismatch", func(t *testing.T) {
		r := New(t, integrationConfig(t))
		at := lineOf(func() { r.Emit("eCount", "k", "invalid") })
		_, err := r.Verify(context.Background())
		if se := setupKind(t, err, "parser"); !strings.Contains(se.Message, "eCount ("+at+")") {
			t.Fatalf("parser message: %v", se)
		}
	})
	t.Run("hot-state", func(t *testing.T) {
		cfg := integrationConfig(t)
		source := `event e; spec Liveness observes e { start hot state Pending { on e do { } } }`
		if err := os.WriteFile(cfg.SpecFiles[0], []byte(source), 0o600); err != nil {
			t.Fatal(err)
		}
		cfg.Specs = []string{"Liveness"}
		cfg.CheckHot = true
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		_, err := r.Verify(context.Background())
		var v *SpecViolation
		if !errors.As(err, &v) || !v.Liveness || !strings.Contains(v.Message, "Pending") || !strings.Contains(v.Error(), "<- events ended here") {
			t.Fatalf("hot completion: %v", err)
		}
	})
}

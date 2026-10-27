package snappeatest

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/crowecawcaw/snappea/go/internal/hook"
	"github.com/crowecawcaw/snappea/go/internal/snappeabin"
	"github.com/crowecawcaw/snappea/go/snappeaemit"
)

// The test binary doubles as a fake snappea and as a child process that emits events.
func init() {
	mode := os.Getenv("SNAPPEATEST_CHILD")
	if mode == "" {
		return
	}
	switch mode {
	case "timeout":
		time.Sleep(2 * time.Second)
		os.Exit(0)
	case "emit":
		snappeaemit.Emit("eCount", "child", 5)
		os.Exit(0)
	case "old", "old-pobserve":
		if len(os.Args) > 1 && os.Args[1] == "--version" {
			fmt.Println("snappea 0.0.1")
			os.Exit(0)
		}
		if mode == "old-pobserve" { // snappea before it had its own observe command line
			fmt.Println("Failed parsing ::\nWas passed main parameter '" + os.Args[2] + "' but no main parameter was defined in your arg class\nUsage: snappea observe [options]\n  --spec")
		} else {
			fmt.Fprint(os.Stderr, "error: unknown option '--unknown-events'\nRun `snappea observe --help` for usage.\n")
		}
		os.Exit(2)
	}
	for n, arg := range os.Args {
		if arg == "--json-report" && n+1 < len(os.Args) {
			data := []byte(`{"exit_code":0,"specs":[]}`)
			switch mode {
			case "malformed":
				data = []byte(`{broken`)
			case "empty-object":
				data = []byte(`{}`)
			case "null":
				data = []byte(`null`)
			case "array":
				data = []byte(`[]`)
			case "unverified-partition":
				data = []byte(`{"exit_code":0,"specs":[{"spec":"S","events_read":1,"partition_keys":1,"verified_keys":0,"errors":[]}]}`)
			case "empty-observation":
				data = []byte(`{"exit_code":0,"specs":[{"spec":"S","events_read":0,"partition_keys":0,"verified_keys":0,"errors":[]}]}`)
			case "events-without-keys":
				data = []byte(`{"exit_code":0,"specs":[{"spec":"S","events_read":1,"partition_keys":0,"verified_keys":0,"errors":[]}]}`)
			case "keys-without-events":
				data = []byte(`{"exit_code":0,"specs":[{"spec":"S","events_read":0,"partition_keys":1,"verified_keys":1,"errors":[]}]}`)
			case "not-checked": // stopped before checking, as with --format text
				data = []byte(`{"exit_code":2}`)
			case "bad-envelope":
				data = []byte(`{"scope":"observed_monitor_suite","schema_version":1,"specs":[],"observations":null}`)
			}
			if err := os.WriteFile(os.Args[n+1], data, 0o600); err != nil {
				fmt.Fprintln(os.Stderr, err)
				os.Exit(2)
			}
		}
	}
	if mode == "not-checked" {
		fmt.Fprintln(os.Stderr, "error: The P specification does not type-check.")
		os.Exit(2)
	}
	fmt.Println("fixture diagnostics")
	os.Exit(0)
}

func fixtureConfig(t *testing.T) Config {
	t.Helper()
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	return Config{Binary: binary, SpecFiles: []string{"unused.p"}, Specs: []string{"S"}, Artifacts: t.TempDir()}
}

func setupKind(t *testing.T, err error, kind string) *SetupError {
	t.Helper()
	var se *SetupError
	if !errors.As(err, &se) || se.Kind != kind {
		t.Fatalf("want a %s SetupError, got %T: %v", kind, err, err)
	}
	return se
}

func lines(t *testing.T, data []byte) []map[string]any {
	t.Helper()
	var out []map[string]any
	for _, line := range bytes.Split(bytes.TrimSpace(data), []byte{'\n'}) {
		var m map[string]any
		if err := json.Unmarshal(line, &m); err != nil {
			t.Fatalf("invalid line %s: %v", line, err)
		}
		out = append(out, m)
	}
	return out
}

// lineOf runs f, written on the caller's line, and returns that file:line.
func lineOf(f func()) string {
	f()
	_, file, line, _ := runtime.Caller(1)
	return fmt.Sprintf("%s:%d", filepath.Base(file), line)
}

func TestEmissionLineFormat(t *testing.T) {
	r := New(t, fixtureConfig(t))
	payload := map[string]any{"value": 1}
	before := time.Now()
	r.EmitAt(time.Unix(10, 123), "tuple", "key", []any{1, "hello"})
	named := lineOf(func() { r.Emit("named", "key", payload) })
	payload["value"] = 99
	r.Emit("scalar", "key", 3)
	r.Emit("unit", "key", nil)
	after := time.Now()
	got := lines(t, r.trace.Bytes())
	if len(got) != 4 {
		t.Fatalf("got %d events", len(got))
	}
	if got[0]["timestamp"] != "1970-01-01T00:00:10.000000123Z" || got[0]["at"] != true || fmt.Sprint(got[0]["payload"]) != "[1 hello]" {
		t.Fatalf("lost precise tuple: %v", got[0])
	}
	var prev time.Time
	for n, e := range got[1:] {
		ts, err := time.Parse(time.RFC3339Nano, e["timestamp"].(string))
		if err != nil || ts.Before(before.Truncate(time.Microsecond)) || ts.After(after) || ts.Before(prev) || e["at"] != nil {
			t.Fatalf("event %d: bad wall-clock time %v (%v)", n, e["timestamp"], err)
		}
		prev = ts
		if e["seq"].(float64) <= got[n]["seq"].(float64) || e["pid"].(float64) != float64(os.Getpid()) {
			t.Fatalf("bad seq or pid: %v", e)
		}
	}
	if got[1]["src"] != named {
		t.Fatalf("src = %v", got[1]["src"])
	}
	if fmt.Sprint(got[1]["payload"]) != "map[value:1]" || got[2]["payload"] != 3.0 || got[3]["payload"] != nil {
		t.Fatalf("payload not snapshotted: %s", r.trace.Bytes())
	}
}

func TestSrcThroughMethodValue(t *testing.T) {
	r := New(t, fixtureConfig(t))
	emit := r.Emit
	here := lineOf(func() { emit("e", "k", nil) })
	if src := lines(t, r.trace.Bytes())[0]["src"]; src != here {
		t.Fatalf("src = %v", src)
	}
}

func TestConcurrentEmission(t *testing.T) {
	r := New(t, fixtureConfig(t))
	var workers sync.WaitGroup
	for n := 0; n < 16; n++ {
		workers.Add(1)
		go func(n int) {
			defer workers.Done()
			for j := 0; j < 16; j++ {
				r.Emit("e", "k", []int{n, j})
			}
		}(n)
	}
	workers.Wait()
	if got := lines(t, r.trace.Bytes()); len(got) != 256 {
		t.Fatalf("lost emissions: %d", len(got))
	}
}

func TestEncodingErrorIsSticky(t *testing.T) {
	r := New(t, fixtureConfig(t))
	r.Emit("bad", "k", make(chan int))
	r.Emit("fine", "k", nil)
	for n := 0; n < 2; n++ {
		result, err := r.Verify(context.Background())
		se := setupKind(t, err, "parser")
		if !strings.Contains(se.Message, "cannot encode the payload of bad") || len(result.Report) != 0 {
			t.Fatalf("lost encoding error or ran the checker: %v", err)
		}
	}
}

func TestEmitAfterCheckFails(t *testing.T) {
	t.Setenv("SNAPPEATEST_CHILD", "empty-observation")
	r := New(t, fixtureConfig(t))
	r.Emit("e", "k", nil)
	r.Verify(context.Background())
	r.Emit("late", "k", nil)
	_, err := r.Verify(context.Background())
	if se := setupKind(t, err, "config"); !strings.Contains(se.Message, "late emitted at recorder_test.go:") {
		t.Fatalf("late emission message: %v", se)
	}
}

func TestMalformedAndMismatchedToolReports(t *testing.T) {
	for _, mode := range []string{"malformed", "missing-spec"} {
		t.Run(mode, func(t *testing.T) {
			t.Setenv("SNAPPEATEST_CHILD", mode)
			r := New(t, fixtureConfig(t))
			r.Emit("e", "k", nil)
			result, err := r.Verify(context.Background())
			setupKind(t, err, "config")
			if !strings.Contains(err.Error(), "report") || !strings.Contains(result.Output, "fixture diagnostics") {
				t.Fatalf("lost malformed tool diagnostics: %v %+v", err, result)
			}
		})
	}
}

func TestFailureBeforeChecking(t *testing.T) {
	t.Setenv("SNAPPEATEST_CHILD", "not-checked")
	r := New(t, fixtureConfig(t))
	r.Emit("e", "k", nil)
	_, err := r.Verify(context.Background())
	if se := setupKind(t, err, "config"); !strings.Contains(se.Message, "does not type-check") || strings.Contains(se.Message, "unexpected report") {
		t.Fatalf("lost snappea's reason: %v", se)
	}
}

func TestOldBinaryNamesVersions(t *testing.T) {
	for mode, want := range map[string]string{"old": "unknown option '--unknown-events'", "old-pobserve": "main parameter '"} {
		t.Run(mode, func(t *testing.T) {
			t.Setenv("SNAPPEATEST_CHILD", mode)
			r := New(t, fixtureConfig(t))
			r.Emit("e", "k", nil)
			_, err := r.Verify(context.Background())
			se := setupKind(t, err, "config")
			if !strings.Contains(se.Message, "snappea 0.0.1") || !strings.Contains(se.Message, "snappeatest "+Version) || !strings.Contains(se.Message, want) || strings.Contains(se.Message, "--help") || strings.Contains(se.Message, "Usage") {
				t.Fatalf("message: %s", se.Message)
			}
		})
	}
}

func TestTimeoutAndMissingBinary(t *testing.T) {
	t.Run("timeout", func(t *testing.T) {
		t.Setenv("SNAPPEATEST_CHILD", "timeout")
		cfg := fixtureConfig(t)
		cfg.Timeout = 25 * time.Millisecond
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		_, err := r.Verify(context.Background())
		setupKind(t, err, "timeout")
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("lost timeout cause: %v", err)
		}
	})
	t.Run("missing", func(t *testing.T) {
		cfg := fixtureConfig(t)
		cfg.Binary = filepath.Join(t.TempDir(), "missing")
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		_, err := r.Verify(context.Background())
		se := setupKind(t, err, "binary_missing")
		if !strings.Contains(se.Message, cfg.Binary) || !strings.Contains(se.Message, "cargo install") {
			t.Fatalf("missing binary message: %v", se)
		}
	})
	t.Run("not-on-path", func(t *testing.T) {
		t.Setenv("SNAPPEA_BIN", "")
		t.Setenv("PATH", t.TempDir())
		t.Setenv("SNAPPEA_DOWNLOAD", "0")
		cfg := fixtureConfig(t)
		cfg.Binary = ""
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		_, err := r.Verify(context.Background())
		setupKind(t, err, "binary_missing")
	})
}

// TestDownloadedBinary serves the test binary (a fake snappea) as a release archive.
func TestDownloadedBinary(t *testing.T) {
	exe, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	body, err := os.ReadFile(exe)
	if err != nil {
		t.Fatal(err)
	}
	name := "snappea"
	if runtime.GOOS == "windows" {
		name = "snappea.exe"
	}
	var archive bytes.Buffer
	if runtime.GOOS == "windows" {
		zw := zip.NewWriter(&archive)
		w, _ := zw.Create(name)
		w.Write(body)
		zw.Close()
	} else {
		gz, _ := gzip.NewWriterLevel(&archive, gzip.BestSpeed)
		tw := tar.NewWriter(gz)
		tw.WriteHeader(&tar.Header{Name: name, Mode: 0o755, Size: int64(len(body)), Typeflag: tar.TypeReg})
		tw.Write(body)
		tw.Close()
		gz.Close()
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if strings.HasPrefix(r.URL.Path, "/gone/") {
			http.NotFound(w, r)
			return
		}
		w.Write(archive.Bytes())
	}))
	defer srv.Close()

	release := snappeabin.Release
	defer func() { snappeabin.Release = release }()
	snappeabin.Release = snappeabin.Source{Version: "1.2.3", BaseURL: srv.URL, CacheDir: t.TempDir()}
	url, err := snappeabin.Release.URL()
	if err != nil {
		t.Skip(err)
	}
	sum := sha256.Sum256(archive.Bytes())
	snappeabin.Release.Sums = hex.EncodeToString(sum[:]) + "  " + path.Base(url) + "\n"

	t.Setenv("SNAPPEA_BIN", "")
	t.Setenv("PATH", t.TempDir())
	t.Setenv("SNAPPEA_DOWNLOAD", "")
	t.Setenv("SNAPPEATEST_CHILD", "empty-observation") // passes when not strict
	verify := func() error {
		cfg := fixtureConfig(t)
		cfg.Binary = ""
		cfg.Strict = Bool(false)
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		_, err := r.Verify(context.Background())
		return err
	}

	working := snappeabin.Release
	snappeabin.Release.BaseURL = srv.URL + "/gone"
	se := setupKind(t, verify(), "binary_missing")
	if !strings.Contains(se.Message, srv.URL+"/gone/v1.2.3/") || !strings.Contains(se.Message, "cargo install") {
		t.Fatalf("download failure message: %s", se.Message)
	}

	snappeabin.Release = working
	if err := verify(); err != nil {
		t.Fatalf("downloaded binary: %v", err)
	}
	if _, err := os.Stat(filepath.Join(working.CacheDir, "1.2.3", name)); err != nil {
		t.Fatalf("not cached: %v", err)
	}
}

func TestNothingToCheckSkipsTheChecker(t *testing.T) {
	cfg := fixtureConfig(t)
	cfg.Binary = filepath.Join(t.TempDir(), "missing")
	cfg.Specs = nil // every spec: not strict
	if _, err := New(t, cfg).Verify(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestModeOff(t *testing.T) {
	t.Setenv("SNAPPEA_MODE", "off")
	cfg := fixtureConfig(t)
	cfg.Binary = filepath.Join(t.TempDir(), "missing")
	r := New(t, cfg)
	r.Emit("e", "k", nil)
	if _, err := r.Verify(context.Background()); err != nil || r.trace.Len() != 0 {
		t.Fatalf("off mode checked or recorded: %v", err)
	}
}

func TestConfiguration(t *testing.T) {
	dir := t.TempDir()
	for _, cfg := range []Config{
		{},
		{SpecFiles: []string{"a"}, Specs: []string{"S", "S"}},
		{SpecFiles: []string{"a"}, Timeout: -1},
		{SpecFiles: []string{"a"}, UnknownEvents: "drop"},
		{SpecFiles: []string{"a"}, Mode: "always"},
		{SpecFiles: []string{filepath.Join(dir, "*.p")}},
		{SpecFiles: []string{"a"}, Specs: []string{"S"}, Exclude: []string{"S"}},
	} {
		if _, err := resolve(cfg); err == nil {
			t.Fatalf("accepted invalid config: %+v", cfg)
		}
	}
	for _, name := range []string{"a.p", "b.p", "c.txt"} {
		os.WriteFile(filepath.Join(dir, name), nil, 0o600)
	}
	s, err := resolve(Config{SpecFiles: []string{dir}, WorkDir: dir, Binary: "./snappea", Exclude: []string{"X"}})
	if err != nil {
		t.Fatal(err)
	}
	if len(s.specFiles) != 2 || !filepath.IsAbs(s.binary) || s.strict || s.unknownEvents != "error" || s.specs != nil || s.exclude[0] != "X" {
		t.Fatalf("defaults: %+v", s)
	}
	s, _ = resolve(Config{SpecFiles: []string{"a.p"}, Specs: []string{"S", "T"}, Exclude: []string{"T"}})
	if !s.strict || len(s.specs) != 1 {
		t.Fatalf("named specs: %+v", s)
	}
	s, _ = resolve(Config{SpecFiles: []string{"a.p"}, Specs: []string{"S"}, Strict: Bool(false)})
	if s.strict {
		t.Fatal("Strict: Bool(false) ignored")
	}
	s, _ = resolve(Config{SpecFiles: []string{"a.p"}, Specs: []string{"S"}, AllowUnobserved: true, ArtifactRoot: "art"})
	if s.strict || len(s.deprecated) != 2 || !strings.HasSuffix(s.artifacts, "art") {
		t.Fatalf("deprecated fields: %+v", s)
	}

	t.Setenv("SNAPPEA_SPEC_FILES", "x.p"+string(os.PathListSeparator)+"y.p")
	t.Setenv("SNAPPEA_ARTIFACTS", "artifacts")
	t.Setenv("SNAPPEA_COVERAGE", "cov.json")
	t.Setenv("SNAPPEA_MODE", "off")
	s, err = resolve(Config{SpecFiles: []string{"ignored.p"}, Artifacts: "other", Coverage: "other.json", WorkDir: dir})
	cwd, _ := os.Getwd()
	if err != nil || len(s.specFiles) != 2 || s.specFiles[0] != filepath.Join(cwd, "x.p") || s.artifacts != filepath.Join(cwd, "artifacts") || s.coverage != filepath.Join(cwd, "cov.json") || !s.off {
		t.Fatalf("environment overrides: %+v %v", s, err)
	}
}

func TestReportValidation(t *testing.T) {
	valid := `{"exit_code":0,"specs":[{"spec":"S","events_read":1,"partition_keys":1,"verified_keys":1,"errors":[]}]}`
	if err := validateReport([]byte(valid), []string{"S"}, true, true); err != nil {
		t.Fatal(err)
	}
	if err := validateReport([]byte(strings.Replace(valid, `"S"`, `"other"`, 1)), nil, true, true); err != nil {
		t.Fatalf("all specs: %v", err)
	}
	failed := func(exit int, kind string) string {
		return fmt.Sprintf(`{"exit_code":%d,"specs":[{"spec":"S","events_read":1,"partition_keys":1,"verified_keys":1,"errors":[{"kind":%q,"message":"m"}]}]}`, exit, kind)
	}
	for _, ok := range []string{failed(1, "spec_violation"), failed(2, "validation"), failed(2, "parser")} {
		if err := validateReport([]byte(ok), []string{"S"}, false, true); err != nil {
			t.Fatalf("rejected %s: %v", ok, err)
		}
	}
	for _, bad := range []string{failed(1, "validation"), failed(3, "validation"), failed(0, "spec_violation")} {
		if err := validateReport([]byte(bad), []string{"S"}, false, true); err == nil {
			t.Fatalf("accepted invalid report: %s", bad)
		}
	}
	if err := validateReport([]byte(failed(1, "spec_violation")), []string{"S"}, true, true); err == nil {
		t.Fatal("accepted a failed report from a successful process")
	}
	for _, bad := range []string{strings.Replace(valid, `"S"`, `"other"`, 1), strings.Replace(valid, `"verified_keys":1`, `"verified_keys":2`, 1), strings.Replace(valid, `"errors":[]`, `"errors":null`, 1), strings.Replace(valid, `"exit_code":0`, `"exit_code":2`, 1), `{"specs":[]}`, `{"exit_code":0,"specs":[{"spec":"S"}]}`} {
		if err := validateReport([]byte(bad), []string{"S"}, true, true); err == nil {
			t.Fatalf("accepted invalid report: %s", bad)
		}
	}
}

func TestClassifyReport(t *testing.T) {
	line := func(event, payload, src string) string {
		data, _ := json.Marshal(fmt.Sprintf(`{"event":%q,"key":"p1","timestamp":"2026-01-01T00:00:00.000000001Z","payload":%s,"src":%q}`, event, payload, src))
		return string(data)
	}
	report := `{"specs":[
	 {"spec":"Inv","errors":[{"kind":"spec_violation","key":"p1","message":"Assertion failure: /work/specs/inv.p:12:5 too many","events":[
	   {"event":"eStocked","log_line":` + line("eStocked", `{"units":1}`, "app.go:10") + `},
	   {"event":"eReserved","log_line":` + line("eReserved", `{"order":"o1","units":2}`, "app.go:20") + `}]}]},
	 {"spec":"Ships","errors":[{"kind":"spec_violation","key":"p2","message":"PImplementation.Ships detected liveness bug in hot state 'Waiting' at the end of program execution.","events":[]}]}]}`
	vs, se := classify([]byte(report), "/work", "/tmp/out")
	if se != nil || len(vs) != 2 {
		t.Fatalf("classify: %v %v", vs, se)
	}
	v := vs[0]
	if v.Spec != "Inv" || v.Key != "p1" || v.Message != "too many" || v.Location != filepath.FromSlash("specs/inv.p:12") || v.Liveness || len(v.Events) != 2 {
		t.Fatalf("violation: %+v", v)
	}
	if e := v.Events[1]; e.Event != "eReserved" || e.Src != "app.go:20" || e.Payload.(map[string]any)["order"] != "o1" || e.Time.IsZero() {
		t.Fatalf("event: %+v", e)
	}
	v.Others = vs[1:]
	want := []string{
		"P spec Inv was broken: too many",
		`  Events (key "p1"):`,
		"      1  eStocked   units=1",
		"      2  eReserved  order=o1 units=2  app.go:20  <- broke the spec here",
		"  Spec: " + filepath.FromSlash("specs/inv.p:12"),
		"  snappea output: /tmp/out",
		`    Ships (key "p2"): it ended in hot state 'Waiting'`,
	}
	for _, w := range want {
		if !strings.Contains(v.Error(), w) {
			t.Fatalf("report lacks %q:\n%s", w, v.Error())
		}
	}
	if !vs[1].Liveness || !strings.Contains(vs[1].Error(), "was still waiting when the events ended") || !strings.Contains(vs[1].Error(), "(none)") {
		t.Fatalf("liveness report:\n%s", vs[1].Error())
	}

	_, se = classify([]byte(`{"specs":[{"spec":"S","errors":[{"kind":"validation","message":"S checked no events"},{"kind":"parser","message":`+line("eCount", `"x"`, "a.go:3")+`,"cause":"cannot read \"x\" as int"}]}]}`), "/work", "")
	if se == nil || se.Kind != "parser" || !strings.Contains(se.Message, `eCount (a.go:3): cannot read "x" as int`) {
		t.Fatalf("parser error: %v", se)
	}
	_, se = classify([]byte(`{"specs":[{"spec":"S","errors":[{"kind":"validation","message":"S checked no events"}]}]}`), "/work", "")
	if se == nil || se.Kind != "validation" {
		t.Fatalf("validation error: %v", se)
	}
}

type shortWriter struct{}

func (shortWriter) Write(p []byte) (int, error) { return 0, nil }

func TestSuiteToolAndOutputFailures(t *testing.T) {
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	t.Setenv("SNAPPEATEST_CHILD", "malformed")
	suite, err := NewSuite(SuiteConfig{Binary: binary})
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "report.json")
	if err = suite.WriteCoverage(context.Background(), path, &bytes.Buffer{}); err == nil || !strings.Contains(err.Error(), "invalid merged coverage JSON") {
		t.Fatalf("accepted malformed merge report: %v", err)
	}
	if err = suite.WriteCoverage(context.Background(), path, shortWriter{}); !errors.Is(err, io.ErrShortWrite) {
		t.Fatalf("lost human output error: %v", err)
	}
	t.Setenv("SNAPPEATEST_CHILD", "timeout")
	suite, err = NewSuite(SuiteConfig{Binary: binary, Timeout: 25 * time.Millisecond})
	if err != nil {
		t.Fatal(err)
	}
	if err = suite.WriteCoverage(context.Background(), path, &bytes.Buffer{}); !errors.Is(err, context.DeadlineExceeded) {
		t.Fatalf("lost coverage timeout cause: %v", err)
	}
}

func TestInvalidMergePreservesPreviousDestination(t *testing.T) {
	binary, err := os.Executable()
	if err != nil {
		t.Fatal(err)
	}
	suite, err := NewSuite(SuiteConfig{Binary: binary})
	if err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "monitor.json")
	previous := []byte("previous successful coverage")
	for _, mode := range []string{"malformed", "empty-object", "null", "array", "bad-envelope"} {
		t.Run(mode, func(t *testing.T) {
			t.Setenv("SNAPPEATEST_CHILD", mode)
			if err := os.WriteFile(path, previous, 0o600); err != nil {
				t.Fatal(err)
			}
			if err := suite.WriteCoverage(context.Background(), path, io.Discard); err == nil {
				t.Fatalf("accepted invalid suite report from %s", mode)
			}
			got, err := os.ReadFile(path)
			if err != nil || !bytes.Equal(got, previous) {
				t.Fatalf("destroyed previous destination: %s %v", got, err)
			}
			entries, err := os.ReadDir(filepath.Dir(path))
			if err != nil || len(entries) != 1 {
				t.Fatalf("leaked merge staging file: %v %v", entries, err)
			}
		})
	}
}

// Even a well-shaped, exit-zero report must honor the core metric invariants.
// Opting out of strict observation permits emptiness, never skipped partitions.
func TestToolSuccessMetricInvariants(t *testing.T) {
	for _, mode := range []string{"unverified-partition", "empty-observation", "keys-without-events", "events-without-keys"} {
		for _, strict := range []bool{true, false} {
			t.Run(fmt.Sprintf("%s/strict=%t", mode, strict), func(t *testing.T) {
				t.Setenv("SNAPPEATEST_CHILD", mode)
				cfg := fixtureConfig(t)
				cfg.Strict = Bool(strict)
				r := New(t, cfg)
				r.Emit("e", "k", nil)
				_, err := r.Verify(context.Background())
				wantSuccess := !strict && mode == "empty-observation"
				if (err == nil) != wantSuccess {
					t.Fatalf("success=%t, want %t: %v", err == nil, wantSuccess, err)
				}
			})
		}
	}
}

func TestArtifactsKeptOnlyOnFailure(t *testing.T) {
	root := t.TempDir()
	t.Run("pass", func(t *testing.T) {
		t.Setenv("SNAPPEATEST_CHILD", "empty-observation")
		cfg := fixtureConfig(t)
		cfg.Artifacts, cfg.Strict = root, Bool(false)
		Start(t, cfg).Emit("e", "k", nil)
	})
	t.Run("fail", func(t *testing.T) {
		t.Setenv("SNAPPEATEST_CHILD", "malformed")
		cfg := fixtureConfig(t)
		cfg.Artifacts = root
		r := New(t, cfg)
		r.Emit("e", "k", nil)
		if _, err := r.Verify(context.Background()); err == nil {
			t.Fatal("malformed report passed")
		}
	})
	entries, err := os.ReadDir(root)
	if err != nil || len(entries) != 1 || !strings.Contains(entries[0].Name(), "fail") {
		t.Fatalf("want only the failed check's folder; got %v %v", entries, err)
	}
}

func TestSnappeaemitReachesTheRecording(t *testing.T) {
	r := New(t, fixtureConfig(t))
	at := lineOf(func() { snappeaemit.Emit("e", "k", 1) })
	got := lines(t, r.trace.Bytes())
	if len(got) != 1 || got[0]["src"] != at || got[0]["payload"] != 1.0 {
		t.Fatalf("snappeaemit event not recorded: %s", r.trace.Bytes())
	}
	t.Setenv("SNAPPEATEST_CHILD", "empty-observation")
	r.Verify(context.Background())
	snappeaemit.Emit("late", "k", nil)
	_, err := r.Verify(context.Background())
	if se := setupKind(t, err, "config"); !strings.Contains(se.Message, "late emitted") {
		t.Fatalf("late snappeaemit event: %v", se)
	}
}

type panicky struct{}

func (panicky) MarshalJSON() ([]byte, error) { panic("boom") }

func TestSnappeaemitPanickingPayloadFailsTheCheck(t *testing.T) {
	r := New(t, fixtureConfig(t))
	before := snappeaemit.Dropped()
	snappeaemit.Emit("bad", "k", panicky{}) // must not panic in the code under test
	snappeaemit.Emit("fine", "k", nil)
	if snappeaemit.Dropped() != before {
		t.Fatalf("the recording should have taken the event, not snappeaemit's drop count")
	}
	_, err := r.Verify(context.Background())
	if se := setupKind(t, err, "parser"); !strings.Contains(se.Message, "cannot encode the payload of bad") || !strings.Contains(se.Message, "boom") {
		t.Fatalf("panicking payload: %v", se)
	}
}

func TestSnappeaemitWithSeveralRecordings(t *testing.T) {
	a, b := New(t, fixtureConfig(t)), New(t, fixtureConfig(t))
	snappeaemit.EmitCtx(a.Context(context.Background()), "e", "k", nil)
	if a.count != 1 || b.count != 0 {
		t.Fatalf("EmitCtx: a=%d b=%d", a.count, b.count)
	}
	snappeaemit.Emit("e", "k", nil)
	for _, r := range []*Recorder{a, b} {
		_, err := r.Verify(context.Background())
		if se := setupKind(t, err, "config"); !strings.Contains(se.Message, "2 recordings were active") {
			t.Fatalf("ambiguous emit: %v", se)
		}
	}
}

func TestSnappeaemitAfterTheTestIsOff(t *testing.T) {
	t.Run("recording", func(t *testing.T) {
		New(t, fixtureConfig(t))
		if !hook.Active() {
			t.Fatal("recording not registered")
		}
	})
	if hook.Active() {
		t.Fatal("recording still registered after its test")
	}
}

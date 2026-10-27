// Package snappeatest checks events emitted by Go applications against P specs, using the
// snappea executable. Each test, subtest or fuzz case starts its own recording; there is
// no process-global recording state.
package snappeatest

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/crowecawcaw/snappea/go/internal/eventline"
	"github.com/crowecawcaw/snappea/go/internal/hook"
)

// Result holds the raw outcome of a check, including on failure. Report is nil when
// snappea wrote no report.
type Result struct {
	Report       json.RawMessage
	Output       string
	ArtifactsDir string
}

// Recorder collects one test's events and checks them once.
type Recorder struct {
	mu       sync.Mutex
	verifyMu sync.Mutex
	s        settings
	dir      string
	events   string
	trace    bytes.Buffer
	count    int
	sticky   *SetupError
	sealed   bool
	done     bool
	result   Result
	setupErr *SetupError
	found    []*SpecViolation
	ignored  map[string]bool
	dirs     []string
	logs     []logInput
}

type logInput struct {
	path  string
	parse func(line string) (Event, bool)
}

// New starts a recording that the test checks explicitly with Check, Verify or
// ExpectViolation, after all application work (workers, child processes) finishes.
// Configuration errors fail the test immediately.
func New(t testing.TB, cfg Config) *Recorder {
	t.Helper()
	s, err := resolve(cfg)
	if err != nil {
		t.Fatalf("%v", &SetupError{Kind: "config", Message: err.Error()})
	}
	for _, msg := range s.deprecated {
		t.Logf("snappeatest: %s", msg)
	}
	root := s.artifacts
	if root == "" {
		root = os.TempDir()
	}
	if err = os.MkdirAll(root, 0o755); err != nil {
		t.Fatalf("snappeatest artifacts: %v", err)
	}
	dir, err := os.MkdirTemp(root, "snappeatest-"+fileSafe(t.Name())+"-")
	if err != nil {
		t.Fatalf("snappeatest artifacts: %v", err)
	}
	r := &Recorder{s: s, dir: dir, events: filepath.Join(dir, "events"), ignored: map[string]bool{}}
	if err = os.Mkdir(r.events, 0o755); err != nil {
		t.Fatalf("snappeatest events folder: %v", err)
	}
	// snappeaemit.Emit in this process reaches the recording until the test ends, so late
	// events fail the check instead of vanishing.
	if !s.off {
		hook.Register(emitHook{r})
	}
	// Registered first, so it runs last: after Start's check.
	t.Cleanup(func() {
		hook.Unregister(emitHook{r})
		if !r.failed() { // keep the folder only for a failed check
			os.RemoveAll(dir)
		}
	})
	return r
}

// emitHook receives snappeaemit events.
type emitHook struct{ r *Recorder }

func (h emitHook) Record(at time.Time, src, event, key string, payload any) {
	h.r.record(at, src, event, key, payload)
}

func (h emitHook) Ambiguous(n int, src, event string) {
	h.r.mu.Lock()
	defer h.r.mu.Unlock()
	h.r.stick("config", fmt.Sprintf("snappeaemit emitted %s at %s while %d recordings were active (parallel tests), so it cannot tell which test it belongs to: "+
		"use snappeaemit.EmitCtx with a context from Recorder.Context, inject Recorder.Emit, or run these tests without t.Parallel", event, src, n))
}

// Context returns parent carrying this recording, so snappeaemit.EmitCtx(ctx, ...) calls
// made with it reach this recording even while other tests record in parallel.
func (r *Recorder) Context(parent context.Context) context.Context {
	return hook.WithRecording(parent, emitHook{r})
}

// Start is New plus a t.Cleanup that checks the recording when the test ends.
// Register cleanups that stop workers or child processes AFTER Start: Go runs
// cleanups in reverse order, so they finish before the check. A test that already
// failed is not checked again.
func Start(t testing.TB, cfg Config) *Recorder {
	t.Helper()
	r := New(t, cfg)
	t.Cleanup(func() {
		t.Helper()
		if t.Failed() {
			r.mu.Lock()
			r.sealed = true
			r.mu.Unlock()
			t.Log("snappeatest: events not checked: the test had already failed")
			return
		}
		r.Check(t)
	})
	return r
}

func fileSafe(name string) string {
	name = strings.Map(func(r rune) rune {
		if r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' || r == '-' {
			return r
		}
		return '_'
	}, name)
	return name[:min(len(name), 80)]
}

// Emit records an event now. payload is a struct or map whose JSON field names match
// the P event's named-tuple fields, a single value, a slice for a tuple, or nil. It is
// encoded immediately, so later changes to it are not recorded. The call site is
// recorded as src for failure reports.
func (r *Recorder) Emit(event, key string, payload any) {
	r.record(time.Time{}, eventline.Caller(1), event, key, payload)
}

// EmitAt records an event at a given time, which orders it among the others.
func (r *Recorder) EmitAt(at time.Time, event, key string, payload any) {
	if at.IsZero() {
		at = time.Unix(0, 0)
	}
	r.record(at, eventline.Caller(1), event, key, payload)
}

func (r *Recorder) record(at time.Time, src, event, key string, payload any) {
	if r.s.off {
		return
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.sealed {
		r.stick("config", fmt.Sprintf("%s emitted at %s after the recording was checked: stop workers and child processes before the check", event, src))
		return
	}
	n, ns := eventline.Stamp(at)
	line, err := encodeSafely(ns, !at.IsZero(), event, key, payload, src, n)
	if err != nil {
		r.stick("parser", fmt.Sprintf("cannot encode the payload of %s at %s: %v; pass a payload encoding/json can marshal", event, src, err))
		return
	}
	r.trace.Write(line)
	r.count++
}

// encodeSafely is eventline.Encode with a panic (a MarshalJSON method that panics) turned
// into an error, so the event fails the check instead of being dropped by Emit's recover.
func encodeSafely(ns int64, at bool, event, key string, payload any, src string, n int64) (line []byte, err error) {
	defer func() {
		if p := recover(); p != nil {
			line, err = nil, fmt.Errorf("panic while encoding: %v", p)
		}
	}()
	return eventline.Encode(ns, at, event, key, payload, src, n)
}

func (r *Recorder) stick(kind, msg string) {
	if r.sticky == nil {
		r.sticky = &SetupError{Kind: kind, Message: msg, ArtifactsDir: r.dir}
	}
}

// EventsDir is the folder child processes write events to. Env sets it for them.
func (r *Recorder) EventsDir() string { return r.events }

// Env is the current environment plus SNAPPEA_EVENTS_DIR, for exec.Cmd.Env, so a child
// process using snappeaemit writes its events into this recording.
func (r *Recorder) Env() []string {
	env := []string{}
	for _, kv := range os.Environ() {
		if !strings.HasPrefix(kv, "SNAPPEA_EVENTS_DIR=") {
			env = append(env, kv)
		}
	}
	return append(env, "SNAPPEA_EVENTS_DIR="+r.events)
}

// AddEventsDir also checks the events folder at path (a container volume, a service
// started before the test), read in the events folder format.
func (r *Recorder) AddEventsDir(path string) {
	r.addInput(path, nil, true)
}

// AddLog also checks the log at path. parse turns a line into an event (Time is
// required) or returns false to skip it; nil means the log is already JSON lines in
// the events folder format.
func (r *Recorder) AddLog(path string, parse func(line string) (Event, bool)) {
	r.addInput(path, parse, false)
}

func (r *Recorder) addInput(path string, parse func(string) (Event, bool), dir bool) {
	if !filepath.IsAbs(path) {
		path = filepath.Join(r.s.workDir, path)
	}
	r.mu.Lock()
	defer r.mu.Unlock()
	if r.sealed {
		r.stick("config", fmt.Sprintf("input %s added after the recording was checked", path))
		return
	}
	if dir {
		r.dirs = append(r.dirs, path)
	} else {
		r.logs = append(r.logs, logInput{path, parse})
	}
}

// Verify seals the recording and checks it once; later calls return the same
// outcome. The error is a *SpecViolation or a *SetupError (use errors.As).
func (r *Recorder) Verify(ctx context.Context) (Result, error) {
	r.verifyMu.Lock()
	defer r.verifyMu.Unlock()
	r.mu.Lock()
	r.sealed = true
	trace := append([]byte(nil), r.trace.Bytes()...)
	count, dirs, logs := r.count, r.dirs, r.logs
	r.mu.Unlock()
	if !r.done {
		r.done = true
		r.result = Result{ArtifactsDir: r.dir}
		r.found, r.setupErr = r.run(ctx, trace, count, dirs, logs)
	}
	result := r.result
	result.Report = append(json.RawMessage(nil), result.Report...)
	if err := r.outcome(); err != nil {
		return result, err
	}
	return result, nil
}

// outcome is the sticky error, the setup error, or the first violation of a spec
// not ignored by ExpectViolation, with the rest as Others.
func (r *Recorder) outcome() error {
	r.mu.Lock()
	sticky := r.sticky
	r.mu.Unlock()
	if sticky != nil {
		return sticky
	}
	if r.setupErr != nil {
		return r.setupErr
	}
	var vs []*SpecViolation
	for _, v := range r.found {
		if !r.ignored[v.Spec] {
			vs = append(vs, v)
		}
	}
	if len(vs) == 0 {
		return nil
	}
	first := *vs[0]
	first.Others = vs[1:]
	return &first
}

func (r *Recorder) failed() bool {
	r.verifyMu.Lock()
	defer r.verifyMu.Unlock()
	return r.done && r.outcome() != nil
}

// Check verifies and reports a failure with t.Errorf. If the test already failed, it
// logs the outcome instead, so the original failure stays first.
func (r *Recorder) Check(t testing.TB) {
	t.Helper()
	if _, err := r.Verify(context.Background()); err != nil {
		if t.Failed() {
			t.Logf("%v", err)
		} else {
			t.Errorf("%v", err)
		}
	}
}

// ExpectViolation is a negative control: it checks the recording now and fails the
// test unless spec was violated with a message matching the regular expression match,
// and no other spec was. Later checks of this recording ignore spec. It returns the
// violation for further assertions.
func (r *Recorder) ExpectViolation(t testing.TB, spec, match string) *SpecViolation {
	t.Helper()
	if r.s.off {
		t.Skip("snappeatest: SNAPPEA_MODE=off")
	}
	re, err := regexp.Compile(match)
	if err != nil {
		t.Fatalf("snappeatest: ExpectViolation match: %v", err)
	}
	_, err = r.Verify(context.Background())
	var v *SpecViolation
	if !errors.As(err, &v) {
		if err == nil {
			t.Fatalf("snappeatest: expected spec %s to be violated (message matching %q); the check passed", spec, match)
		}
		t.Fatalf("snappeatest: expected spec %s to be violated; got:\n%v", spec, err)
	}
	r.verifyMu.Lock()
	r.ignored[spec] = true
	r.verifyMu.Unlock()
	var found *SpecViolation
	var others []string
	for _, x := range append([]*SpecViolation{v}, v.Others...) {
		switch {
		case x.Spec != spec:
			others = append(others, x.Error())
		case found == nil && re.MatchString(x.Message):
			y := *x
			y.Others = nil
			found = &y
		}
	}
	if len(others) > 0 {
		t.Fatalf("snappeatest: expected only spec %s to be violated; also:\n%s", spec, strings.Join(others, "\n\n"))
	}
	if found == nil {
		t.Fatalf("snappeatest: spec %s was violated, but not with a message matching %q:\n%v", spec, match, v)
	}
	return found
}

func (r *Recorder) run(ctx context.Context, trace []byte, count int, dirs []string, logs []logInput) ([]*SpecViolation, *SetupError) {
	s := r.s
	setup := func(kind, msg string, err error) *SetupError {
		return &SetupError{Kind: kind, Message: msg, ArtifactsDir: r.dir, Err: err}
	}
	own := filepath.Join(r.dir, "events.jsonl")
	if err := os.WriteFile(own, trace, 0o600); err != nil {
		return nil, setup("config", "write events: "+err.Error(), err)
	}
	r.mu.Lock()
	sticky := r.sticky
	r.mu.Unlock()
	if s.off || sticky != nil {
		return nil, nil
	}
	inputs := []string{own, r.events}
	inputs = append(inputs, dirs...)
	for n, l := range logs {
		if l.parse == nil {
			inputs = append(inputs, l.path)
			continue
		}
		path := filepath.Join(r.dir, fmt.Sprintf("log-%d-%s.jsonl", n, fileSafe(filepath.Base(l.path))))
		if err := convertLog(l, path); err != nil {
			return nil, setup("config", err.Error(), err)
		}
		inputs = append(inputs, path)
	}
	if count == 0 && !s.strict && len(dirs) == 0 && len(logs) == 0 && isEmptyDir(r.events) {
		return nil, nil // nothing to check
	}
	binary, missing := findBinary(ctx, s.binary)
	if missing != nil {
		missing.ArtifactsDir = r.dir
		return nil, missing
	}

	// The spec files are positional sources. Each log goes through -l, so snappea never
	// takes one for a source.
	args := append([]string{"observe"}, s.specFiles...)
	if len(s.specs) > 0 {
		args = append(args, "--spec", strings.Join(s.specs, ","))
	} else {
		args = append(args, "--all-specs")
		if len(s.exclude) > 0 {
			args = append(args, "--exclude-specs", strings.Join(s.exclude, ","))
		}
	}
	report := filepath.Join(r.dir, "report.json")
	for _, input := range inputs {
		args = append(args, "-l", input)
	}
	// The console output is kept for people (snappea-output.txt); the result is read
	// from the report.
	args = append(args, "--format", "text", "--json-report", report, "--unknown-events", s.unknownEvents)
	if s.strict {
		args = append(args, "--strict")
	}
	if s.checkHot {
		args = append(args, "--check-hot")
	}
	ctx, cancel := context.WithTimeout(ctx, s.timeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, binary, args...)
	cmd.Dir = s.workDir
	cmd.WaitDelay = time.Second
	output, cmdErr := cmd.CombinedOutput()
	r.result.Output = string(output)
	_ = os.WriteFile(filepath.Join(r.dir, "snappea-output.txt"), []byte(fmt.Sprintf("%s %q\n\n%s", binary, args, output)), 0o600)
	if ctx.Err() != nil {
		return nil, setup("timeout", fmt.Sprintf("snappea did not finish within %v", s.timeout), errors.Join(ctx.Err(), cmdErr))
	}
	data, readErr := os.ReadFile(report)
	if readErr == nil {
		r.result.Report = data
	}
	if readErr != nil || failedBeforeChecking(data) {
		var exitErr *exec.ExitError
		if cmdErr != nil && !errors.As(cmdErr, &exitErr) {
			return nil, setup("config", "cannot run snappea: "+cmdErr.Error(), cmdErr)
		}
		return nil, setup("config", commandError(string(output), binary), cmdErr)
	}
	if err := validateReport(data, s.specs, cmdErr == nil, s.strict); err != nil {
		return nil, setup("config", fmt.Sprintf("snappea wrote an unexpected report (is it snappea %s?): %v", Version, err), err)
	}
	if s.suite != nil {
		s.suite.collect(data)
	}
	if err := addCoverage(s, data); err != nil {
		return nil, setup("config", "suite coverage: "+err.Error(), err)
	}
	return classify(data, s.workDir, r.dir)
}

func isEmptyDir(dir string) bool {
	entries, err := os.ReadDir(dir)
	return err == nil && len(entries) == 0
}

func convertLog(l logInput, out string) error {
	in, err := os.Open(l.path)
	if err != nil {
		return err
	}
	defer in.Close()
	var buf bytes.Buffer
	sc := bufio.NewScanner(in)
	sc.Buffer(make([]byte, 64*1024), 16*1024*1024)
	for n := 1; sc.Scan(); n++ {
		ev, ok := l.parse(sc.Text())
		if !ok {
			continue
		}
		where := fmt.Sprintf("%s:%d", filepath.Base(l.path), n)
		if ev.Time.IsZero() {
			return fmt.Errorf("AddLog parser returned %s without a Time, at %s", ev.Event, where)
		}
		if ev.Src == "" {
			ev.Src = where
		}
		line, err := eventline.Encode(ev.Time.UnixNano(), false, ev.Event, ev.Key, ev.Payload, ev.Src, 0)
		if err != nil {
			return fmt.Errorf("cannot encode %s from %s: %v", ev.Event, where, err)
		}
		buf.Write(line)
	}
	if err := sc.Err(); err != nil {
		return err
	}
	return os.WriteFile(out, buf.Bytes(), 0o600)
}

// commandError is snappea's console output without its pointer to the help, or a note
// when the binary is too old for an option this helper passes. An older snappea, with
// PObserve's command line, rejects the positional spec files as a "main parameter".
func commandError(output, binary string) string {
	text := strings.TrimSpace(output)
	for _, tail := range []string{"\nRun `snappea observe --help`", "\nUsage:"} {
		if i := strings.Index(text, tail); i >= 0 {
			text = strings.TrimSpace(text[:i])
		}
	}
	text = strings.TrimSpace(strings.TrimPrefix(text, "Failed parsing ::"))
	if strings.Contains(text, "error: unknown option '") || strings.Contains(text, "main parameter '") {
		version, _ := exec.Command(binary, "--version", "--format", "text").Output()
		return fmt.Sprintf("%s\n%s (%s) lacks an option snappeatest %s needs; use snappea from the same snappea release",
			text, binary, strings.TrimSpace(string(version)), Version)
	}
	return "snappea could not check the events:\n" + text
}

// failedBeforeChecking reports a result without specs: snappea stopped before checking
// (unreadable sources, a bad option value), and its console output says why.
func failedBeforeChecking(data []byte) bool {
	var report struct {
		Specs json.RawMessage `json:"specs"`
	}
	return json.Unmarshal(data, &report) == nil && report.Specs == nil
}

// validateReport checks a report's shape and that its exit_code (snappea's: 0 no error,
// 1 a spec violation, 2 any other error) agrees with its errors and the process.
func validateReport(data []byte, names []string, commandOK, strict bool) error {
	var report struct {
		Exit  *int `json:"exit_code"`
		Specs []struct {
			Name     string  `json:"spec"`
			Events   *uint64 `json:"events_read"`
			Keys     *uint64 `json:"partition_keys"`
			Verified *uint64 `json:"verified_keys"`
			Errors   []struct {
				Kind    string `json:"kind"`
				Message string `json:"message"`
			} `json:"errors"`
		} `json:"specs"`
	}
	if err := json.Unmarshal(data, &report); err != nil {
		return fmt.Errorf("decode observation report: %w", err)
	}
	if report.Exit == nil || report.Specs == nil || (len(names) > 0 && len(report.Specs) != len(names)) {
		return errors.New("missing report status or unexpected selected specs")
	}
	wanted := map[string]bool{}
	for _, name := range names {
		wanted[name] = true
	}
	seen := map[string]bool{}
	hasErrors, hasViolation := false, false
	for _, s := range report.Specs {
		if seen[s.Name] || (len(names) > 0 && !wanted[s.Name]) {
			return fmt.Errorf("unexpected or duplicate report spec %q", s.Name)
		}
		seen[s.Name] = true
		if s.Events == nil || s.Keys == nil || s.Verified == nil || *s.Verified > *s.Keys || *s.Events < *s.Keys || s.Errors == nil {
			return fmt.Errorf("missing or invalid metrics/errors for %s", s.Name)
		}
		if *report.Exit == 0 {
			if (*s.Events == 0) != (*s.Keys == 0) {
				return fmt.Errorf("successful report has inconsistent event/partition emptiness for %s", s.Name)
			}
			if *s.Verified != *s.Keys {
				return fmt.Errorf("successful report left partitions unverified for %s", s.Name)
			}
			if strict && *s.Events == 0 {
				return fmt.Errorf("strict successful report checked no events for %s", s.Name)
			}
		}
		for _, e := range s.Errors {
			if e.Kind == "" || e.Message == "" {
				return errors.New("report error without kind or message")
			}
			hasErrors = true
			hasViolation = hasViolation || e.Kind == "spec_violation"
		}
	}
	if *report.Exit < 0 || *report.Exit > 2 || (*report.Exit == 0) == hasErrors || (*report.Exit == 1 && !hasViolation) || commandOK != (*report.Exit == 0) {
		return fmt.Errorf("inconsistent process/report status %d", *report.Exit)
	}
	return nil
}

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
	"sort"
	"strings"
	"sync"
	"testing"
	"time"
)

// Main runs the package's tests and then prints suite spec coverage once: what every
// check together covered of the spec monitors, and the states and handlers no check
// reached. Use it as
//
//	func TestMain(m *testing.M) { snappeatest.Main(m) }
//
// By default the coverage is only printed; no file is written. Set Config.Coverage
// (or SNAPPEA_COVERAGE) to a path to also keep it as JSON: the checks are merged into
// that file together with what is already there. SNAPPEA_COVERAGE=off (or
// Config.Coverage "off") turns suite coverage off. A failure to print the default
// report is a warning; a failure to write a configured path fails the run.
//
// Without Main there is no default report, and checks with a coverage path merge into
// the file as each one completes, which is slower.
func Main(m *testing.M) {
	collector.mu.Lock()
	collector.inMain = true
	collector.mu.Unlock()
	code := m.Run()
	if err := flushCoverage(context.Background(), os.Stdout, os.Stderr); err != nil {
		fmt.Fprintln(os.Stderr, "snappeatest:", err)
		if code == 0 {
			code = 1
		}
	}
	os.Exit(code)
}

type pending struct {
	s       settings
	reports []json.RawMessage
}

var collector struct {
	mu       sync.Mutex
	inMain   bool
	paths    map[string]*pending
	implicit *pending // checks without a coverage path, for the default report
}

// addCoverage adds a check's report to the suite coverage file named by s.coverage,
// or, with Main and no path, to the default report printed at the end of the run.
func addCoverage(s settings, report []byte) error {
	if s.coverageOff {
		return nil
	}
	collector.mu.Lock()
	if s.coverage == "" {
		if collector.inMain {
			if collector.implicit == nil {
				collector.implicit = &pending{s: s}
			}
			collector.implicit.reports = append(collector.implicit.reports, append(json.RawMessage(nil), report...))
		}
		collector.mu.Unlock()
		return nil
	}
	if collector.inMain {
		if collector.paths == nil {
			collector.paths = map[string]*pending{}
		}
		p := collector.paths[s.coverage]
		if p == nil {
			p = &pending{s: s}
			collector.paths[s.coverage] = p
		}
		p.reports = append(p.reports, append(json.RawMessage(nil), report...))
		collector.mu.Unlock()
		return nil
	}
	collector.mu.Unlock()
	return mergeCoverage(context.Background(), s, s.coverage, []json.RawMessage{report}, io.Discard)
}

// flushCoverage merges the collected reports: into each configured path, then the
// checks without a path into a printed-only report. That report's errors go to warn as
// a warning, since nobody asked for it.
func flushCoverage(ctx context.Context, human, warn io.Writer) error {
	collector.mu.Lock()
	paths, implicit := collector.paths, collector.implicit
	collector.paths, collector.implicit = nil, nil
	collector.mu.Unlock()
	if implicit != nil && len(implicit.reports) > 0 {
		if err := printCoverage(ctx, implicit.s, implicit.reports, human); err != nil {
			fmt.Fprintf(warn, "snappeatest: warning: no suite spec coverage report: %s\n", oneLine(err))
		}
	}
	keys := make([]string, 0, len(paths))
	for path := range paths {
		keys = append(keys, path)
	}
	sort.Strings(keys)
	var errs []error
	for _, path := range keys {
		p := paths[path]
		if err := mergeCoverage(ctx, p.s, path, p.reports, human); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

// printCoverage writes the merged text summary of reports to human, writing no file.
func printCoverage(ctx context.Context, s settings, reports []json.RawMessage, human io.Writer) error {
	data, err := json.Marshal(reports)
	if err != nil {
		return err
	}
	binary, missing := findBinary(ctx, s.binary)
	if missing != nil {
		return missing
	}
	ctx, cancel := context.WithTimeout(ctx, s.timeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, binary, "coverage", "merge-observe", "--stdin", "--format", "text")
	cmd.Dir = s.workDir
	cmd.Stdin = bytes.NewReader(data)
	cmd.WaitDelay = time.Second
	output, err := cmd.CombinedOutput()
	if ctx.Err() != nil {
		err = errors.Join(err, ctx.Err())
	}
	if err != nil {
		return fmt.Errorf("snappea coverage merge-observe: %w: %s", err, bytes.TrimSpace(output))
	}
	_, err = fmt.Fprintf(human, "\nSpec coverage of all checks (SNAPPEA_COVERAGE=<file.json> keeps it, =off hides it):\n%s", output)
	return err
}

// oneLine joins the lines of err's message, for a one-line warning.
func oneLine(err error) string {
	return strings.Join(strings.Fields(err.Error()), " ")
}

// mergeCoverage merges reports into the suite file at path under an exclusive lock,
// so several test processes (go test ./...) can share it.
func mergeCoverage(ctx context.Context, s settings, path string, reports []json.RawMessage, human io.Writer) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	unlock, err := lockFile(path + ".lock")
	if err != nil {
		return fmt.Errorf("lock %s: %w", path, err)
	}
	defer unlock()
	inputs := reports
	existing, err := os.ReadFile(path)
	merging := err == nil && len(bytes.TrimSpace(existing)) > 0
	if merging {
		inputs = append([]json.RawMessage{existing}, reports...)
	}
	data, err := json.Marshal(inputs)
	if err != nil {
		return err
	}
	if err = runMerge(ctx, s.binary, s.workDir, s.timeout, data, path, human); err != nil && merging {
		return fmt.Errorf("%w\n(an old %s from other spec sources cannot be merged: delete it before the run)", err, path)
	}
	return err
}

// SuiteConfig configures an explicit Suite. Use the same snappea version for checking and
// merging.
type SuiteConfig struct {
	Binary  string
	WorkDir string
	Timeout time.Duration // Zero means 30 seconds.
}

// Suite collects the reports of checks whose Config.Suite points to it, for
// WriteCoverage. Most packages need only Config.Coverage and Main instead.
type Suite struct {
	mu      sync.Mutex
	cfg     SuiteConfig
	reports []json.RawMessage
}

func NewSuite(cfg SuiteConfig) (*Suite, error) {
	var err error
	cfg.WorkDir, err = filepath.Abs(cfg.WorkDir)
	if err != nil {
		return nil, err
	}
	if cfg.Timeout < 0 {
		return nil, errors.New("negative coverage timeout")
	}
	if cfg.Timeout == 0 {
		cfg.Timeout = 30 * time.Second
	}
	cfg.Binary = resolveBinary(cfg.Binary, cfg.WorkDir)
	return &Suite{cfg: cfg, reports: []json.RawMessage{}}, nil
}

func (s *Suite) collect(data []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.reports = append(s.reports, append(json.RawMessage(nil), data...))
}

// WriteCoverage merges the collected reports with snappea coverage merge-observe,
// replacing jsonPath (relative to SuiteConfig.WorkDir) and writing the summary to
// human. Call it after m.Run, and keep m.Run's exit code.
func (s *Suite) WriteCoverage(ctx context.Context, jsonPath string, human io.Writer) error {
	if jsonPath == "" {
		return errors.New("coverage JSON path is required")
	}
	if human == nil {
		return errors.New("coverage human output writer is required")
	}
	s.mu.Lock()
	data, err := json.Marshal(s.reports)
	s.mu.Unlock()
	if err != nil {
		return err
	}
	if !filepath.IsAbs(jsonPath) {
		jsonPath = filepath.Join(s.cfg.WorkDir, jsonPath)
	}
	if err = os.MkdirAll(filepath.Dir(jsonPath), 0o755); err != nil {
		return err
	}
	return runMerge(ctx, s.cfg.Binary, s.cfg.WorkDir, s.cfg.Timeout, data, jsonPath, human)
}

// runMerge merges beside the destination and publishes only after all checks succeed,
// so a failed merge never destroys an earlier report.
func runMerge(ctx context.Context, binary, workDir string, timeout time.Duration, data []byte, jsonPath string, human io.Writer) error {
	binary, missing := findBinary(ctx, binary)
	if missing != nil {
		return missing
	}
	pending, err := os.CreateTemp(filepath.Dir(jsonPath), ".snappeatest-coverage-*.json")
	if err != nil {
		return err
	}
	pendingPath := pending.Name()
	defer os.Remove(pendingPath)
	if err = pending.Close(); err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	cmd := exec.CommandContext(ctx, binary, "coverage", "merge-observe", "--stdin", "--format", "text", "--json-report", pendingPath)
	cmd.Dir = workDir
	cmd.Stdin = bytes.NewReader(data)
	cmd.WaitDelay = time.Second
	output, commandErr := cmd.CombinedOutput()
	written, writeErr := human.Write(output)
	if writeErr == nil && written != len(output) {
		writeErr = io.ErrShortWrite
	}
	if ctx.Err() != nil {
		commandErr = errors.Join(commandErr, ctx.Err())
	}
	if err = errors.Join(commandErr, writeErr); err != nil {
		return fmt.Errorf("observed monitor coverage merge: %w\n%s", err, output)
	}
	report, err := os.ReadFile(pendingPath)
	if err != nil {
		return fmt.Errorf("read merged coverage: %w\n%s", err, output)
	}
	var envelope struct {
		Scope        string            `json:"scope"`
		Version      *int              `json:"schema_version"`
		Specs        []json.RawMessage `json:"specs"`
		Observations []json.RawMessage `json:"observations"`
	}
	if err = json.Unmarshal(report, &envelope); err != nil {
		return fmt.Errorf("invalid merged coverage JSON: %w\n%s", err, report)
	}
	if envelope.Scope != "observed_monitor_suite" || envelope.Version == nil || *envelope.Version != 1 || envelope.Specs == nil || envelope.Observations == nil {
		return fmt.Errorf("invalid merged coverage envelope: %s", report)
	}
	if err = os.Rename(pendingPath, jsonPath); err != nil {
		return fmt.Errorf("publish merged coverage: %w", err)
	}
	return nil
}

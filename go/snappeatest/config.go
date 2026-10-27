package snappeatest

import (
	"context"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/crowecawcaw/snappea/go/internal/snappeabin"
)

// Version is the snappea release this helper belongs to ("dev" between releases). Use a
// snappea binary of the same version: the helper passes options older binaries lack.
const Version = snappeabin.Version

// Config selects the specs and options for one recording. Relative paths resolve
// against WorkDir (default: the working directory, which `go test` sets to the
// package folder). Environment variables override the fields they name; their
// relative paths resolve against the process working directory.
type Config struct {
	// SpecFiles are P files, folders (every .p file in them) or glob patterns.
	// SNAPPEA_SPEC_FILES (separated like PATH) overrides it.
	SpecFiles []string
	// Specs to check. Empty means every spec in SpecFiles.
	Specs []string
	// Exclude leaves specs out.
	Exclude []string
	// Strict requires every selected spec to check at least one event. Default: true
	// when Specs is set, false when Specs defaults to every spec. Set it with Bool.
	Strict *bool
	// CheckHot also fails a key whose spec ends in a hot state (a liveness check).
	CheckHot bool
	// UnknownEvents is what to do with event names no spec file declares: "ignore",
	// "warn" or "error" (default).
	UnknownEvents string
	// Timeout for the checker. Zero means 30 seconds.
	Timeout time.Duration
	// Artifacts is the folder that keeps the files of failed checks (events, snappea
	// output, report). Default: a temporary folder. SNAPPEA_ARTIFACTS overrides it.
	// Passing checks leave nothing behind.
	Artifacts string
	// Coverage is the suite coverage JSON path. Each check's observed monitor
	// coverage is merged into it, once at the end with Main, otherwise as each check
	// completes, under a file lock. Empty means Main only prints the suite coverage;
	// "off" turns it off. SNAPPEA_COVERAGE (a path or "off") overrides it.
	Coverage string
	// Mode is "auto" or "marked" (the same in Go: tests opt in by calling Start) or
	// "off" (recordings do nothing and pass). SNAPPEA_MODE overrides it.
	Mode string
	// Binary is the snappea executable. Default: SNAPPEA_BIN, then snappea on PATH, then the
	// release binary of this module's version, downloaded once into the user cache
	// (os.UserCacheDir()/snappea/<version>) and verified against the module's checksums.
	// SNAPPEA_DOWNLOAD=0 turns the download off.
	Binary string
	// WorkDir is the folder relative Config paths resolve against.
	WorkDir string
	// Suite collects the check's coverage for Suite.WriteCoverage.
	Suite *Suite

	// Deprecated: use Strict: Bool(false).
	AllowUnobserved bool
	// Deprecated: use Artifacts.
	ArtifactRoot string
}

// Bool returns a pointer to v, for Config.Strict.
func Bool(v bool) *bool { return &v }

// settings is a Config with defaults, environment overrides and absolute paths.
type settings struct {
	specFiles     []string
	specs         []string
	exclude       []string
	strict        bool
	checkHot      bool
	unknownEvents string
	timeout       time.Duration
	artifacts     string
	coverage      string
	coverageOff   bool
	off           bool
	binary        string
	workDir       string
	suite         *Suite
	deprecated    []string
}

func resolve(cfg Config) (settings, error) {
	var s settings
	var err error
	if s.workDir, err = filepath.Abs(cfg.WorkDir); err != nil {
		return s, err
	}
	cwd, err := os.Getwd()
	if err != nil {
		return s, err
	}
	abs := func(base, p string) string {
		if p == "" || filepath.IsAbs(p) {
			return p
		}
		return filepath.Join(base, p)
	}

	files, base := cfg.SpecFiles, s.workDir
	if env := os.Getenv("SNAPPEA_SPEC_FILES"); env != "" {
		files, base = filepath.SplitList(env), cwd
	}
	for _, f := range files {
		if f == "" {
			continue
		}
		f = abs(base, f)
		if info, err := os.Stat(f); err == nil && info.IsDir() {
			matches, _ := filepath.Glob(filepath.Join(f, "*.p"))
			if len(matches) == 0 {
				return s, fmt.Errorf("spec folder %s has no .p files", f)
			}
			s.specFiles = append(s.specFiles, matches...)
		} else if strings.ContainsAny(f, "*?[") {
			matches, err := filepath.Glob(f)
			if err != nil || len(matches) == 0 {
				return s, fmt.Errorf("spec file pattern %s matches nothing", f)
			}
			s.specFiles = append(s.specFiles, matches...)
		} else {
			s.specFiles = append(s.specFiles, f)
		}
	}
	if len(s.specFiles) == 0 {
		return s, fmt.Errorf("no spec files: set Config.SpecFiles or SNAPPEA_SPEC_FILES")
	}

	excluded := map[string]bool{}
	for _, name := range cfg.Exclude {
		excluded[name] = true
	}
	seen := map[string]bool{}
	for _, name := range cfg.Specs {
		if name == "" || seen[name] {
			return s, fmt.Errorf("empty or duplicate spec name %q", name)
		}
		seen[name] = true
		if !excluded[name] {
			s.specs = append(s.specs, name)
		}
	}
	if len(cfg.Specs) > 0 && len(s.specs) == 0 {
		return s, fmt.Errorf("Exclude leaves none of Specs %v", cfg.Specs)
	}
	s.exclude = append([]string(nil), cfg.Exclude...)

	s.strict = len(s.specs) > 0
	if cfg.AllowUnobserved {
		s.strict = false
		s.deprecated = append(s.deprecated, "Config.AllowUnobserved is deprecated: use Strict: snappeatest.Bool(false)")
	}
	if cfg.Strict != nil {
		s.strict = *cfg.Strict
	}
	s.checkHot = cfg.CheckHot

	switch s.unknownEvents = cfg.UnknownEvents; s.unknownEvents {
	case "":
		s.unknownEvents = "error"
	case "ignore", "warn", "error":
	default:
		return s, fmt.Errorf("UnknownEvents %q: want ignore, warn or error", cfg.UnknownEvents)
	}

	if cfg.Timeout < 0 {
		return s, fmt.Errorf("negative Timeout")
	}
	s.timeout = cfg.Timeout
	if s.timeout == 0 {
		s.timeout = 30 * time.Second
	}

	if cfg.ArtifactRoot != "" {
		s.deprecated = append(s.deprecated, "Config.ArtifactRoot is deprecated: use Artifacts")
	}
	if env := os.Getenv("SNAPPEA_ARTIFACTS"); env != "" {
		s.artifacts = abs(cwd, env)
	} else if cfg.Artifacts != "" {
		s.artifacts = abs(s.workDir, cfg.Artifacts)
	} else {
		s.artifacts = abs(s.workDir, cfg.ArtifactRoot)
	}
	if env := os.Getenv("SNAPPEA_COVERAGE"); env == "off" {
		s.coverageOff = true
	} else if env != "" {
		s.coverage = abs(cwd, env)
	} else if cfg.Coverage == "off" {
		s.coverageOff = true
	} else {
		s.coverage = abs(s.workDir, cfg.Coverage)
	}

	mode := cfg.Mode
	if env := os.Getenv("SNAPPEA_MODE"); env != "" {
		mode = env
	}
	switch mode {
	case "", "auto", "marked":
	case "off":
		s.off = true
	default:
		return s, fmt.Errorf("mode %q: want auto, marked or off", mode)
	}

	s.binary = resolveBinary(cfg.Binary, s.workDir)
	s.suite = cfg.Suite
	return s, nil
}

func resolveBinary(binary, dir string) string {
	if binary == "" {
		binary = os.Getenv("SNAPPEA_BIN")
	}
	if binary == "" {
		binary = "snappea"
	}
	if !filepath.IsAbs(binary) && strings.ContainsAny(binary, `/\`) {
		return filepath.Join(dir, binary)
	}
	return binary
}

const installHelp = `install snappea with
  cargo install snappea
or download a release binary from https://github.com/crowecawcaw/snappea/releases,
then put it on PATH or set SNAPPEA_BIN to its absolute path`

// findBinary returns the snappea executable to run, or a SetupError of kind
// binary_missing. A bare "snappea" (the default) is looked up on PATH, then downloaded.
func findBinary(ctx context.Context, binary string) (string, *SetupError) {
	var err error
	if strings.ContainsAny(binary, `/\`) {
		_, err = os.Stat(binary)
	} else {
		var path string
		if path, err = exec.LookPath(binary); err == nil {
			binary = path
		}
	}
	if err == nil {
		return binary, nil
	}
	if binary != "snappea" {
		return "", &SetupError{Kind: "binary_missing", Message: fmt.Sprintf("no snappea binary at %q: %s", binary, installHelp), Err: err}
	}
	path, err := snappeabin.Release.Get(ctx)
	if err != nil {
		return "", &SetupError{Kind: "binary_missing", Message: fmt.Sprintf("SNAPPEA_BIN is not set, snappea is not on PATH, and %s.\n%s", err, installHelp), Err: err}
	}
	return path, nil
}

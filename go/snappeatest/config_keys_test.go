package snappeatest

import (
	"os"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"testing"
)

// Config fields that are Go API options, not concepts of docs/language-support.md.
var goOnlyFields = map[string]bool{"Binary": true, "WorkDir": true, "Suite": true}

func TestConfigFieldsMatchTheConceptsTable(t *testing.T) {
	doc, err := os.ReadFile("../../docs/language-support.md")
	if err != nil {
		t.Fatal(err)
	}
	section := strings.SplitN(strings.SplitN(string(doc), "### Concepts", 2)[1], "\n#", 2)[0]
	var concepts []string
	for _, m := range regexp.MustCompile("(?m)^\\| `(\\w+)` \\|").FindAllStringSubmatch(section, -1) {
		concepts = append(concepts, camel(m[1]))
	}
	var fields []string
	typ := reflect.TypeOf(Config{})
	for i := 0; i < typ.NumField(); i++ {
		if name := typ.Field(i).Name; !goOnlyFields[name] {
			fields = append(fields, name)
		}
	}
	sort.Strings(concepts)
	sort.Strings(fields)
	if !reflect.DeepEqual(concepts, fields) {
		t.Fatalf("Config fields %v, Concepts table %v", fields, concepts)
	}
}

// camel turns spec_files into SpecFiles.
func camel(key string) string {
	var b strings.Builder
	for _, part := range strings.Split(key, "_") {
		b.WriteString(strings.ToUpper(part[:1]) + part[1:])
	}
	return b.String()
}

package io.github.crowecawcaw.snappea;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Set;
import java.util.regex.Pattern;
import java.util.stream.Collectors;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.assertEquals;

/** The config file keys match the Concepts table in docs/language-support.md. */
class ConfigKeysTest {
    @Test void configKeysMatchTheConceptsTable() throws Exception {
        var doc = Files.readString(Path.of("../docs/language-support.md"));
        var section = doc.split("### Concepts", 2)[1].split("\n#", 2)[0];
        Set<String> concepts = Pattern.compile("(?m)^\\| `(\\w+)` \\|").matcher(section).results()
                .map(m -> m.group(1)).collect(Collectors.toSet());
        assertEquals(concepts, SnappeaConfig.KEYS);
    }
}

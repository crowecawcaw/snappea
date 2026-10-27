"""Contract-selected semantic faults; no grader dependence on driver report text."""

def variants(source):
    retry_reapplies = source.replace(
        '        result = memo[p.command.id];',
        '        result = memo[p.command.id];\n'
        '        if (result.accepted) { value = p.command.value; }', 1)
    rejected_not_memoized = source.replace(
        '      if (p.command.id in memo) {',
        '      if (p.command.id in memo && memo[p.command.id].accepted) {', 1)
    retry_reports_current_version = source.replace(
        '        result = memo[p.command.id];',
        '        result = memo[p.command.id];\n'
        '        result.version = version;', 1)
    new_write_wrong_value = source.replace(
        '          value = p.command.value;',
        '          value = p.command.value + 1;', 1)
    alternative = source.replace('  var memo: map[int, Outcome];',
        '  var known: set[int];\n  var savedAccepted: map[int, bool];\n  var savedVersion: map[int, int];')
    alternative = alternative.replace('p.command.id in memo', 'p.command.id in known')
    alternative = alternative.replace('result = memo[p.command.id];',
        'result = (accepted = savedAccepted[p.command.id], version = savedVersion[p.command.id]);')
    alternative = alternative.replace('memo[p.command.id] = result;',
        'known += (p.command.id);\n        savedAccepted[p.command.id] = result.accepted;\n        savedVersion[p.command.id] = result.version;')
    result = {'correct': source, 'correct_alt': alternative,
              'retry_reapplies': retry_reapplies,
              'rejected_not_memoized': rejected_not_memoized,
              'retry_reports_current_version': retry_reports_current_version,
              'new_write_wrong_value': new_write_wrong_value}
    assert len(set(result.values())) == len(result)
    return result

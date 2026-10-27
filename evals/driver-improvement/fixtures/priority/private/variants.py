"""Independent contract-derived algorithm faults; no monitor/driver changes."""


def replace_once(source, old, new):
    assert source.count(old) == 1, (old, source.count(old))
    return source.replace(old, new, 1)


def variants(correct):
    tie = replace_once(correct,
        '(rank == best && admission[key] < admission[chosen])',
        '(rank == best && admission[key] > admission[chosen])')
    reprio = replace_once(correct,
        '''      } else if (c.op == 1) {
        if (c.id in active) {
          accepted = true;
          priorities[c.id] = c.priority;
        }''',
        '''      } else if (c.op == 1) {
        if (c.id in active) {
          accepted = true;
          priorities[c.id] = c.priority;
          admission[c.id] = serial;
          serial = serial + 1;
        }''')
    old_position = replace_once(correct,
        '          admission[c.id] = serial;',
        '          if (!(c.id in admission)) { admission[c.id] = serial; }')
    absolute = replace_once(correct,
        '          rank = priorities[key];',
        '          rank = priorities[key];\n          if (rank < 0) { rank = 0 - rank; }')
    duplicate = replace_once(correct,
        '''          active += (c.id);
        }
      } else if (c.op == 1)''',
        '''          active += (c.id);
        } else { priorities[c.id] = c.priority; }
      } else if (c.op == 1)''')
    cancel_no_effect = replace_once(correct,
        '''      } else if (c.op == 2) {
        if (c.id in active) {
          accepted = true;
          active -= (c.id);
        }''',
        '''      } else if (c.op == 2) {
        if (c.id in active) {
          accepted = true;
        }''')
    reverse = replace_once(correct, '      var found: bool;',
        '      var found: bool;\n      var entries: seq[int];\n      var index: int;')
    reverse = replace_once(reverse, '        foreach (key in active) {',
        '''        foreach (key in active) { entries += (sizeof(entries), key); }
        index = sizeof(entries);
        while (index > 0) {
          index = index - 1;
          key = entries[index];''')
    result = {'correct': correct, 'correct_alt': reverse,
        'ties_lifo': tie, 'reprioritize_renumbers': reprio,
        'readmit_keeps_old_position': old_position,
        'absolute_priority': absolute, 'duplicate_overwrites_priority': duplicate,
        'cancel_no_effect': cancel_no_effect}
    assert len(set(result.values())) == len(result)
    return result

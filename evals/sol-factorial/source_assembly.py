"""Keep the public event schema verbatim, allowing only extra type/event declarations."""
import re


def _type_expression(text):
    """Conservative P type-expression recognizer; executable syntax is rejected."""
    tokens=[];position=0
    pattern=re.compile(r'\s*([A-Za-z_]\w*|[\[\](),:])')
    while position<len(text):
        if not text[position:].strip():break
        match=pattern.match(text,position)
        if not match:raise ValueError('only type expressions are allowed in extra declarations')
        tokens.append(match.group(1));position=match.end()
    offset=0
    def accept(token):
        nonlocal offset
        if offset<len(tokens) and tokens[offset]==token:offset+=1;return True
        return False
    def expect(token):
        if not accept(token):raise ValueError('invalid extra type expression')
    def parse():
        nonlocal offset
        if accept('('):
            while True:
                if offset+1<len(tokens) and re.fullmatch(r'[A-Za-z_]\w*',tokens[offset]) and tokens[offset+1]==':':offset+=2
                parse()
                if not accept(','):break
            expect(')');return
        if offset>=len(tokens) or not re.fullmatch(r'[A-Za-z_]\w*',tokens[offset]):raise ValueError('invalid extra type expression')
        name=tokens[offset];offset+=1
        if accept('['):
            if name not in ('seq','set','map'):raise ValueError('only seq, set and map accept type arguments')
            parse()
            if name=='map':expect(',');parse()
            expect(']')
    parse()
    if offset!=len(tokens):raise ValueError('unexpected tokens in extra type expression')


def assemble(original,candidate):
    if candidate is None:return original
    if candidate.count(original)!=1:raise ValueError('public event schema must remain verbatim exactly once')
    before,after=candidate.split(original)
    extras=before+'\n'+after
    # No comments here: this deliberately refuses suppression or hidden code.
    if '//' in extras or '/*' in extras:raise ValueError('extra declarations cannot contain comments or suppressions')
    names=re.findall(r'\b(?:type|event)\s+([A-Za-z_]\w*)',original)
    chunks=extras.split(';')
    if chunks[-1].strip():raise ValueError('extra declarations must end with a semicolon')
    for text in chunks[:-1]:
        if not text.strip():continue
        match=re.fullmatch(r'\s*(type|event)\s+([A-Za-z_]\w*)\s*(.*?)\s*',text,re.S)
        if not match:raise ValueError('only extra type/event declarations are supported')
        kind,name,body=match.groups()
        if name in names:raise ValueError('duplicate or conflicting extra declaration: '+name)
        names.append(name)
        if kind=='type':
            if not body.startswith('='):raise ValueError('extra type declaration needs =')
            _type_expression(body[1:])
        elif body:
            if not body.startswith(':'):raise ValueError('invalid extra event declaration')
            _type_expression(body[1:])
    return candidate


def corrected_events(public,candidate):
    return assemble(public.read_text(),candidate.read_text() if candidate.is_file() else None)

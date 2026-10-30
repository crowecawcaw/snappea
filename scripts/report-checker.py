#!/usr/bin/env python3
"""Compare frozen checker benchmark results, keeping missed bug sites visible."""
import argparse
import gzip
import json
from pathlib import Path


def read(path):
    text=gzip.open(path,'rt').read() if str(path).endswith('.gz') else Path(path).read_text()
    return json.loads(text)


def compare(before,after,strategy='auto'):
    for key in ['suite_version','manifest_sha256']:
        if before[key]!=after[key]:raise ValueError(f'incompatible {key}')
    for key in ['budget','steps','seed_start','seeds']:
        if before['settings'][key]!=after['settings'][key]:raise ValueError(f'incompatible {key}')
    b={r['case']:r for r in before['summary'] if r['strategy']==strategy}
    a={r['case']:r for r in after['summary'] if r['strategy']==strategy}
    if set(a)!=set(b):raise ValueError('different case selection')
    sources=lambda x:{c['name']:c['source_sha256'] for c in x['cases']}
    if sources(before)!=sources(after):raise ValueError('different fixture sources')
    return [(name,b[name],a[name]) for name in sorted(b)]


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('before');p.add_argument('after');p.add_argument('--strategy',default='auto');a=p.parse_args()
    before,after=read(a.before),read(a.after);rows=compare(before,after,a.strategy)
    for label,data in [('before',before),('after',after)]:
        selected=[r for r in data['rows'] if r['strategy']==a.strategy and r['expected']]
        print(f"{label}: all-bug completion {sum(r['schedules_to_all'] is not None for r in selected)}/{len(selected)}; sites {sum(len(r['first']) for r in selected)}/{sum(len(r['expected']) for r in selected)}; time-censored {sum(r['time_censored'] for r in selected)}")
    print('| Case | All before | All after | p90 before | p90 after | Recall before | Recall after |\n|---|---:|---:|---:|---:|---:|---:|')
    for name,b,c in rows:
        def tail(x):return x['p90'] if x['p90'] is not None else 'censored'
        print(f"| {name} | {b['completed']}/{b['runs']} | {c['completed']}/{c['runs']} | {tail(b)} | {tail(c)} | {b['recall']:.1%} | {c['recall']:.1%} |")

if __name__=='__main__':main()

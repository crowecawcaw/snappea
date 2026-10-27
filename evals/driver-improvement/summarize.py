#!/usr/bin/env python3
"""Summarize verified frozen trials; retain raw and corrected selection outcomes."""
import json
from pathlib import Path

from common import digest, write_json
from grade import verify

ROOT = Path(__file__).resolve().parent


def main():
    records = []
    by_fixture = {}
    prompts = set()
    for cohort in ['sol', 'luna']:
        for run in sorted((ROOT/'runs').glob(cohort+'-*-*01')):
            meta = verify(run)
            original = json.loads((run/'grading/grade.json').read_text())
            corrected_path = run/'supplemental-selection-v2/grade.json'
            corrected = json.loads(corrected_path.read_text())
            provenance = json.loads((corrected_path.parent/'provenance.json').read_text())
            assert provenance['original_grade_sha256'] == digest(run/'grading/grade.json')
            assert corrected['results'] == original['results']
            assert corrected['baseline_results'] == original['baseline_results']
            assert provenance['checker_rerun'] is False
            prompts.add((run/'prompt.txt').read_text().replace(run.name, 'RUN'))
            pair = (meta['public_sha256'], meta['private_sha256'], meta['harness_sha256'],
                    meta['binary_sha256'], meta['image'], meta['limits'])
            if meta['fixture'] in by_fixture:
                assert by_fixture[meta['fixture']] == pair
            else:
                by_fixture[meta['fixture']] = pair
            faults = [r for r in corrected['results'] if not r['expected_correct']]
            records.append({
                'run_id': run.name, 'cohort': cohort, 'fixture': meta['fixture'],
                'reports': meta['reports'], 'model': meta['model'],
                'reasoning_effort': meta['reasoning_effort'], 'skill': meta['skill'],
                'calls': meta['calls'], 'seconds': meta['seconds'],
                'metadata_sha256': digest(run/'metadata.json'),
                'submission_manifest_sha256': digest(run/'submission-manifest.json'),
                'transcript_sha256': digest(run/'tool-transcript.jsonl'),
                'original_grade_sha256': digest(run/'grading/grade.json'),
                'corrected_grade_sha256': digest(corrected_path),
                'selection_correction_changed_eligibility':
                    original['summary']['valid'] != corrected['summary']['valid'],
                'original_summary': original['summary'], 'summary': corrected['summary'],
                'surviving_faults': [r['variant'] for r in faults if r['status'] == 'accepted'],
                'non_assertion_fault_outcomes': [
                    {'variant': r['variant'], 'status': r['status']}
                    for r in faults if r['status'] not in ['accepted', 'assertion_failure']],
                'policy': corrected['policy'],
            })
    assert len(records) == 12 and len(prompts) == 1
    assert all(r['skill'] is None for r in records)
    data = {
        'version': 'driver-improvement-pilot-v2', 'trials': records,
        'matching': 'Within each fixture, public/private inputs, original harness, binary, image, budgets and normalized prompts match across all four runs.',
        'binary_sha256': next(iter(by_fixture.values()))[3],
        'image': next(iter(by_fixture.values()))[4],
        'limits': next(iter(by_fixture.values()))[5],
        'scope': 'Two separate exploratory cohorts, one run per fixture/report cell. Posthoc probes are separate; no effect-size claim.',
    }
    write_json(ROOT/'results.json', data)
    print(json.dumps({'trials': len(records), 'paired_inputs_verified': True,
                      'selection_corrections': sum(r['selection_correction_changed_eligibility'] for r in records)}))


if __name__ == '__main__':
    main()

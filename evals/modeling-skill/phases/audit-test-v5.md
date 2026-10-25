# Frozen testing-phase v5 source audit

Scope: source-first review of frozen `phase-test-v5-01` and `phase-test-v5-02` submissions and tool transcripts. No v5 grading files were read. Both transcripts show the saved tests passing after each author added `events.p` to `spec_files` (v5-01 calls 6-8; v5-02 calls 5-7).

## Corrected source findings

Correction: my previous audit incorrectly said v5-02 lacked an expiry test and v5-01 lacked a stale release against a successor. I had misread the combined source output. I reopened both complete frozen files and verified these cases by line number.

Both suites now cover all three targeted conditions:

- **Read immediately after every rejected write.** V5-01 reads after the no-lease, wrong-owner, wrong-token, expired, and stale-token writes before any later success can hide a state change (`test_lease.py:87-100,122-125,129-134`). V5-02 does likewise after wrong-owner/wrong-token writes and after the expired and stale-token writes (`test_lease.py:84-90,130-143`).
- **Isolate the expiry guard before handoff.** V5-01 advances Alice’s lease to `expires_at == now`, then calls `write("alice", old_token, 99)` before reacquisition and reads the unchanged value (`test_lease.py:118-127`). V5-02 advances Carol’s lease by 2 then 1 ticks, calls `write("carol", first, 99)` with the same owner and current token after expiry but before reacquisition, and reads the retained value (`test_lease.py:116-135`). These cases keep owner and token checks valid while testing the expiry condition.
- **Reject stale release while a successor is live.** V5-01 reacquires as Alice with a new token, then releases with Alice’s old token and checks the successor lease remains visible (`test_lease.py:129-136`). The successor has the same client identity but a different lease token. V5-02 releases Alice’s lease, lets Bob acquire, then calls `release("alice", first)` and immediately checks Bob’s live lease (`test_lease.py:98-112`); this also tests the stale old owner against a different successor client.

The tests also isolate active wrong-owner and wrong-token write/release guards with public `read` or `peek` observations. Both wrappers record actual public arguments and results, including public reads and peeks, and check each history against the P monitor with `strict=True` and unknown-event errors. The monitor permits strictly increasing token gaps.

## V4 context and conclusion

The supplied v4 summary reports one 9/9 run and one 8/9 run; the surviving fault silently changed value on a rejected write before later successful work overwrote it. V5 places reads before that overwrite, and both source files include the isolated expiry and live-successor release histories missing from earlier versions. The supplied v5 result says both submissions reached 9/9. The source supports that improvement: both now exercise the identified rejection-side-effect, expiry-guard, and stale-successor classes. These remain bounded serial API histories, not concurrency evidence.

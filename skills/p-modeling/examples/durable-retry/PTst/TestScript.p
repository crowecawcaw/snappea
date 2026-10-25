test tcDurableRetry [main=RetryDriver]:
  assert RetryContract in { RetryDriver, DurableService };

test tcSplitCommitRetry [main=SplitRetryDriver]:
  assert RetryContract in { SplitRetryDriver, SplitCommitService };

test tcCompletedReplay [main=CompletedReplayDriver]:
  assert RetryContract in { CompletedReplayDriver, DurableService };

test tcReappliedReplay [main=ReappliedReplayDriver]:
  assert RetryContract in { ReappliedReplayDriver, ReapplyReplayService };

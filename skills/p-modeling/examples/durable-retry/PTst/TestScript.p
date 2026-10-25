test tcDurableRetry [main=RetryDriver]:
  assert RetryContract in { RetryDriver, DurableService };

test tcSplitCommitRetry [main=SplitRetryDriver]:
  assert RetryContract in { SplitRetryDriver, SplitCommitService };

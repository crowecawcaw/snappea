test tcLiveChangesDuringBackfill [main=MigrationCase]:
  assert MigrationContract in { MigrationCase, MigrationStore, BackfillWorker, LiveWriter };

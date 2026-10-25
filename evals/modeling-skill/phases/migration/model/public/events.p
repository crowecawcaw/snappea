event ePrepare: (token: int, watermark: int);
event eApply: (token: int, applied: bool);
event eWrite: (item: string, value: int, revision: int);
event eDelete: (item: string, revision: int);
event eCutover: (accepted: bool);
event eRead: (item: string, present: bool, value: int);
event eNewRow: (item: string, revision: int, deleted: bool, encoded: int);
event eReady: (complete: bool);

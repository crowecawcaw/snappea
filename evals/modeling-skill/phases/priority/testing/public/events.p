event eSubmit: (item_id: string, priority: int, accepted: bool);
event ePop: (item_id: string);
event eReprioritize: (item_id: string, priority: int, accepted: bool);
event eCancel: (item_id: string, accepted: bool);

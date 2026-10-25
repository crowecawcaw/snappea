event eAdd: (timestamp: int, value: int, accepted: bool);
event eWatermark: (watermark: int);
event eOutput: (window_start: int, window_end: int, total: int, revision: int, final: bool);
event eEndCall;

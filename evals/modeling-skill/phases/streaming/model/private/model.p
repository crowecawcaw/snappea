machine StreamingActor {
  start state Run {
    entry {
      announce eAdd, (timestamp = 1, value = 4, accepted = true);
      announce eEndCall;
      announce eAdd, (timestamp = 11, value = -2, accepted = true);
      announce eEndCall;
      announce eWatermark, (watermark = 10,);
      announce eOutput, (window_start = 0, window_end = 10, total = 4, revision = 0, final = false);
      announce eEndCall;
      announce eAdd, (timestamp = 8, value = 3, accepted = true);
      announce eOutput, (window_start = 0, window_end = 10, total = 7, revision = 1, final = false);
      announce eEndCall;
      announce eAdd, (timestamp = 14, value = 5, accepted = true);
      announce eEndCall;
      announce eWatermark, (watermark = 15,);
      announce eOutput, (window_start = 0, window_end = 10, total = 7, revision = 2, final = true);
      announce eEndCall;
      announce eAdd, (timestamp = 9, value = 100, accepted = false);
      announce eEndCall;
      announce eWatermark, (watermark = 20,);
      announce eOutput, (window_start = 10, window_end = 20, total = 3, revision = 0, final = false);
      announce eEndCall;
      announce eWatermark, (watermark = 25,);
      announce eOutput, (window_start = 10, window_end = 20, total = 3, revision = 1, final = true);
      announce eEndCall;
    }
  }
}


machine StreamingCase {
  start state Start {
    entry { new StreamingActor(); }
  }
}

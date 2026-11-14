// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// The cluster behaves like one key-value store: replaying the puts in the order of their
// first replies gives every get its answer. A put reply from any server means the put
// committed; later replies for the same request repeat it and are not replayed.
spec SafetySynchronization observes ClientPutRequest, ClientGetRequest, RaftGetResponse, RaftPutResponse {
  var store: map[KeyT, ValueT];
  var gets: map[Client, map[TransactionId, KeyT]];
  var puts: map[Client, map[TransactionId, (key: KeyT, value: ValueT)]];
  var answered: map[Client, set[TransactionId]];

  start state Listening {
    entry {
      store = default(map[KeyT, ValueT]);
      gets = default(map[Client, map[TransactionId, KeyT]]);
      puts = default(map[Client, map[TransactionId, (key: KeyT, value: ValueT)]]);
      answered = default(map[Client, set[TransactionId]]);
    }

    // a server forwards a request unchanged, so every copy records the same entry
    on ClientGetRequest do (payload: ClientGetRequestData) {
      noteClient(payload.client);
      gets[payload.client][payload.transactionId] = payload.key;
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      noteClient(payload.client);
      puts[payload.client][payload.transactionId] = (key=payload.key, value=payload.value);
    }

    on RaftGetResponse do (payload: RaftGetResponseData) {
      var key: KeyT;
      assert payload.client in gets && payload.transactionId in gets[payload.client],
      format("get reply for transaction {0} of {1}, which sent no such get", payload.transactionId, payload.client);
      if (!(payload.transactionId in answered[payload.client])) {
        answered[payload.client] += (payload.transactionId);
        key = gets[payload.client][payload.transactionId];
        if (key in store) {
          assert payload.success && payload.value == store[key],
          format("get {0} by {1} of key {2}: expected value {3}, got found={4} value={5}", payload.transactionId, payload.client, key, store[key], payload.success, payload.value);
        }
        else {
          assert !payload.success,
          format("get {0} by {1} of key {2}: no answered put wrote it, got value {3}", payload.transactionId, payload.client, key, payload.value);
        }
      }
    }

    on RaftPutResponse do (payload: RaftPutResponseData) {
      assert payload.client in puts && payload.transactionId in puts[payload.client],
      format("put reply for transaction {0} of {1}, which sent no such put", payload.transactionId, payload.client);
      assert payload.key == puts[payload.client][payload.transactionId].key,
      format("put reply for key {0}, but the request wrote key {1}", payload.key, puts[payload.client][payload.transactionId].key);
      if (!(payload.transactionId in answered[payload.client])) {
        answered[payload.client] += (payload.transactionId);
        store[payload.key] = puts[payload.client][payload.transactionId].value;
      }
    }
  }

  fun noteClient(client: Client) {
    if (!(client in answered)) {
      answered[client] = default(set[TransactionId]);
      gets[client] = default(map[TransactionId, KeyT]);
      puts[client] = default(map[TransactionId, (key: KeyT, value: ValueT)]);
    }
  }
}

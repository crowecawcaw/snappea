// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

spec SafetySynchronization observes ClientPutRequest, ClientGetRequest, RaftGetResponse, RaftPutResponse, BecomeLeader {
  var localKVStore: KVStore;
  var getRequestMap: map[Client, map[TransactionId, KeyT]];
  var putRequestMap: map[Client, map[TransactionId, (key: KeyT, value: ValueT)]];
  var seenId: map[Client, set[int]];
  var respondedId : map[Client, set[int]];
  var currentLeader: Server;
  var term: TermId;

  start state Init {
    entry {
      localKVStore = newStore();
      getRequestMap = default(map[Client, map[TransactionId, KeyT]]);
      putRequestMap = default(map[Client, map[TransactionId, (key: KeyT, value: ValueT)]]);
      seenId = default(map[Client, set[int]]);
      respondedId = default(map[Client, set[int]]);
      currentLeader = default(Server);
      term = -1;
      goto Listening;
    }
  }

  state Listening {
    // snappea-lint: allow unused-payload-field -- this rule follows the current leader; the log specs check its log
    on BecomeLeader do (payload: BecomeLeaderData) {
      if (payload.term > term) {
        currentLeader = payload.leader;
        term = payload.term;
      }
    }

    on ClientGetRequest do (payload: ClientGetRequestData) {
      if (!(payload.client in keys(seenId))) {
        seenId[payload.client] = default(set[int]);
      }

      if (!(payload.client in keys(respondedId))) {
        respondedId[payload.client] = default(set[int]);
      }

      if (!(payload.client in keys(getRequestMap))) {
        getRequestMap[payload.client] = default(map[TransactionId, KeyT]);
      }

      if (payload.client == payload.sender && !(payload.transactionId in seenId[payload.client])) {
        seenId[payload.client] += (payload.transactionId);
      }

      getRequestMap[payload.client][payload.transactionId] = payload.key;
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      if (!(payload.client in keys(seenId))) {
        seenId[payload.client] = default(set[int]);
      }

      if (!(payload.client in keys(respondedId))) {
        respondedId[payload.client] = default(set[int]);
      }

      if (!(payload.client in keys(putRequestMap))) {
        putRequestMap[payload.client] = default(map[TransactionId, (key: KeyT, value: ValueT)]);
      }

      if (payload.client == payload.sender && !(payload.transactionId in seenId[payload.client])) {
        seenId[payload.client] += (payload.transactionId);
      }

      putRequestMap[payload.client][payload.transactionId] = (key=payload.key, value=payload.value);
    }

    on RaftGetResponse do (payload: RaftGetResponseData) {
      var executionResult: ExecutionResult;
      checkResponseValid(payload.client, payload.sender, payload.transactionId);
      if (!(payload.client in keys(respondedId)) && currentLeader == payload.sender) {
        respondedId[payload.client] += (payload.transactionId);
        executionResult = executeGet(localKVStore, getRequestMap[payload.client][payload.transactionId]);
        assert executionResult.result.success == payload.success, format("Inconsistent status: {0}", getRequestMap[payload.client][payload.transactionId]);
        assert executionResult.result.value == payload.value, format("Inconsistent Get result! Expected {0}, got {1}", executionResult.result.value, payload.value);
      }
    }

    on RaftPutResponse do (payload: RaftPutResponseData) {
      var executionResult: ExecutionResult;
      checkResponseValid(payload.client, payload.sender, payload.transactionId);
      if (!(payload.client in keys(respondedId)) && currentLeader == payload.sender) {
        respondedId[payload.client] += (payload.transactionId);
        assert payload.client in keys(putRequestMap);
        assert payload.key == putRequestMap[payload.client][payload.transactionId].key,
        format("put response for key {0}, but the request wrote key {1}", payload.key, putRequestMap[payload.client][payload.transactionId].key);
        executionResult = executePut(localKVStore, putRequestMap[payload.client][payload.transactionId].key, putRequestMap[payload.client][payload.transactionId].value);
        localKVStore = executionResult.newState;
      }
    }
  }

  fun checkResponseValid(client: Client, sender: Server, Id: TransactionId) {
    assert currentLeader != default(Server), "Leader is un-initialized but got an RaftResponse!";
    if (currentLeader == sender) {
      assert client in keys(seenId), format("Responding to a client that has not sent any request: {0}", client);
      assert Id in seenId[client], format("Responding to a non-existing transactionId {0} sent by {1}", Id, client);
    }
  }
}

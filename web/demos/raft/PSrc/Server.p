// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// A node in the Raft cluster. The implementation follows the Raft paper (Ongaro and
// Ousterhout, 2014). This implementation does not include log compaction / cluster
// reconfiguration.
type ServerId = int;
type TransactionId = int;

// Response message to the client
type RaftResponseData = (sender: Server, client: Client, transactionId: TransactionId, result: Result);
// event RaftResponse: RaftResponseData;
type RaftPutResponseData = (sender: Server, client: Client, transactionId: TransactionId, key: KeyT);
type RaftGetResponseData = (sender: Server, client: Client, success: bool, transactionId: TransactionId, value: ValueT);
// type RaftGetErrorData = (sender: Server, client: Client, transactionId: TransactionId, key: KeyT);
event RaftPutResponse: RaftPutResponseData;
event RaftGetResponse: RaftGetResponseData;
// event RaftGetError: RaftGetErrorData;


// Server initialization message
event ServerInit: (myId: ServerId, cluster: set[Server], viewServer: View);

// Vote request and response
type TermId = int;
type LogIndex = int;
type RequestVoteData = (term: TermId, candidate: Server, lastLogIndex: LogIndex, lastLogTerm: TermId);
event RequestVote: RequestVoteData;
type RequestVoteReplyData = (sender: Server, term: TermId, voteGranted: bool);
event RequestVoteReply: RequestVoteReplyData;

// Heartbeat message and response
type AppendEntriesData = (term: TermId, leader: Server, prevLogIndex: LogIndex,
prevLogTerm: TermId, entries: seq[ServerLog], leaderCommit: LogIndex);

event AppendEntries: AppendEntriesData;
type AppendEntriesReplyData = (sender: Server, term: TermId, success: bool, matchedIndex: LogIndex);
event AppendEntriesReply: AppendEntriesReplyData;

// Resetting a server (to model crashes)
event Reset;

// events to notify the server role change to the view server
event ViewChangedFollower: Server;
event ViewChangedLeader: Server;
event ViewChangedCandidate: Server;

// server logs
type ServerLog = (term: TermId, key: KeyT, value: ValueT, client: Client, transactionId: TransactionId);

fun TermLT(t1: TermId, t2: TermId): bool {
  return t1 < t2;
}

fun IsSuccess(s: bool): bool {
  return s;
}

fun IsFailure(s: bool): bool {
  return !s;
}

// A node in the Raft cluster
machine Server {
  // the application state
  var kvStore: KVStore;
  // the leader of the cluster
  var leader: Server;
  // size of the cluster
  var clusterSize: int;
  // nodes in the cluster
  var peers: set[Server];
  // the view service
  var viewServer: View;

  // Leader state (volatile)
  var nextIndex: map[Server, int];
  var matchIndex: map[Server, int];
  // Leader state (persistent)
  var currentTerm: int;
  var logs: seq[ServerLog];

  // States for voting
  var votedFor: Server;
  var votesReceived: set[Server];

  // all servers
  var commitIndex: int;
  var lastApplied: int;

  var logNotifyTimestamp: int;

  // var heartbeatTimer: Timer;
  // var clientRequestQueue: seq[ClientRequestData];
  var requestSequenceNumber: int;
  var clientPutRequestQueue: seq[(sequenceNumber: int, payload: ClientPutRequestData)];
  var clientGetRequestQueue: seq[(sequenceNumber: int, payload: ClientGetRequestData)];
  var clientPutRequestCache: map[Client, map[int, RaftPutResponseData]];
  var clientGetRequestCache: map[Client, map[int, RaftGetResponseData]];

  start state Init {
    entry {
    }

    on ServerInit do (setup: (myId: ServerId, cluster: set[Server], viewServer: View)) {
      kvStore = newStore();
      assert this in setup.cluster, "Server should be in the cluster";
      clusterSize = sizeof(setup.cluster);
      peers = setup.cluster;
      nextIndex = default(map[Server, int]);
      matchIndex = default(map[Server, int]);
      votesReceived = default(set[Server]);
      logs = default(seq[ServerLog]);
      // clientRequestQueue = default(seq[ClientRequestData]);
      clientPutRequestQueue = default(seq[(sequenceNumber: int, payload: ClientPutRequestData)]);
      clientGetRequestQueue = default(seq[(sequenceNumber: int, payload: ClientGetRequestData)]);
      requestSequenceNumber = 0;
      clientPutRequestCache = default(map[Client, map[int, RaftPutResponseData]]);
      clientGetRequestCache = default(map[Client, map[int, RaftGetResponseData]]);
      viewServer = setup.viewServer;

      currentTerm = 0;
      logNotifyTimestamp = 0;
      votedFor = default(Server);
      commitIndex = -1;
      lastApplied = -1;
      leader = default(Server);
      goto Follower;
    }

    ignore Reset, ElectionTimeout, HeartbeatTimeout;
  }

  state Follower {
    entry {
      // reset the leader upon being a follower
      leader = default(Server);
      printLog("Become follower");
      send viewServer, ViewChangedFollower, this;
    }

    on Shutdown do {
      goto FinishedServing;
    }

    on Reset do {
      reset();
    }

    on RequestVote do (payload: RequestVoteData) {
      handleRequestVote(payload);
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      // check if the request has been processed
      respondOrForwardPut(payload);
    }

    on ClientGetRequest do (payload: ClientGetRequestData) {
      respondOrForwardGet(payload);
    }

    on AppendEntries do (payload: AppendEntriesData) {
      if (payload.term >= currentTerm) {
        printLog(format("heartbeat updated term from {0} to {1}", currentTerm, payload.term));
        if (leader != default(Server) && payload.leader != leader) {
          votedFor = default(Server);
        }

        leader = payload.leader;
        updateTermAndVote(payload.term);
      }

      handleAppendEntries(payload);
      if (leader != default(Server)) {
        // propagate client requests to the leader
        drainRequestQueuesTo(leader);
      }
    }

    on ElectionTimeout do {
      printLog("Election timeout");
      goto Candidate;
    }

    on HeartbeatTimeout do {
      notifyViewLog();
    }

    ignore RequestVoteReply, AppendEntriesReply;
  }

  state Candidate {
    entry {
      var peer: Server;
      var lastTerm: int;
      printLog("Become candidate");
      send viewServer, ViewChangedCandidate, this;
      currentTerm = currentTerm + 1;
      votedFor = default(Server);
      votesReceived = default(set[Server]);
      votesReceived += (this);
      // if there is only 1 server, no need to request vote
      if (sizeof(votesReceived) > clusterSize / 2) {
        goto Leader;
      }
      else {
        printLog(format("RequestVote with term {0}", currentTerm));
        broadcastRequest(this, peers, RequestVote,
        (term=currentTerm,
        candidate=this,
        lastLogIndex=lastLogIndex(logs),
        lastLogTerm=lastLogTerm(logs)));
      }
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      if (checkClientRequestCache(payload.client, payload.transactionId)) {
        return;
      }

      clientPutRequestQueue += (sizeof(clientPutRequestQueue), (sequenceNumber=requestSequenceNumber, payload=payload));
      requestSequenceNumber = requestSequenceNumber + 1;
    }

    on Shutdown do {
      goto FinishedServing;
    }

    on AppendEntries do (payload: AppendEntriesData) {
      if (payload.term >= currentTerm) {
        // if there is a leader, convert to follower
        printLog(format("Received heartbeat from a leader with higher term {0}", payload.term));
        updateTermAndVote(payload.term);
        handleAppendEntries(payload);
        goto Follower;
      }
      else {
        printLog(format("Received smaller term heartbeat from {0}", payload.leader));
        handleAppendEntries(payload);
      }
    }

    on AppendEntriesReply do (payload: AppendEntriesReplyData) {
      if (payload.term > currentTerm) {
        printLog(format("Received AppendEntriesReply with higher term {0}", payload.term));
        updateTermAndVote(payload.term);
        goto Follower;
      }
    }

    on RequestVoteReply do (payload: RequestVoteReplyData) {
      if (payload.term > currentTerm) {
        // if there is a vote with higher term, convert to follower
        updateTermAndVote(payload.term);
        goto Follower;
      }
      else if (payload.voteGranted && payload.term == currentTerm) {
        // if the vote is granted for me in this term, count it.
        votesReceived += (payload.sender);
        printLog(format("vote granted by {0}; now vote={1}", payload.sender, votesReceived));
        if (sizeof(votesReceived) > clusterSize / 2) {
          // if majority votes are received, become a leader
          printLog(format("majority votes: {0}", votesReceived));
          goto Leader;
        }
      }
      else {
        printLog(format("Received rejection/invalid reply: {0}", payload));
      }
    }

    on RequestVote do (payload: RequestVoteData)  {
      if (payload.term > currentTerm) {
        updateTermAndVote(payload.term);
        handleRequestVote(payload);
        goto Follower;
      }
      else {
        send payload.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
      }
    }

    on Reset do {
      reset();
    }

    on ElectionTimeout do {
      goto Candidate;
    }

    on HeartbeatTimeout do {
      notifyViewLog();
    }

    ignore ClientGetRequest;
  }

  state Leader {
    entry {
      send viewServer, ViewChangedLeader, this;
      leader = this;
      // instantiate the nextIndex and matchIndex
      nextIndex = fillMap(this, nextIndex, peers, sizeof(logs));
      matchIndex = fillMap(this, matchIndex, peers, -1);
      announce BecomeLeader, (term=currentTerm, leader=this, log=logs, commitIndex=commitIndex);
      // first exhaust the client request queu on its own
      drainRequestQueuesTo(this);
      printLog(format("Become leader with Log={0}, commiteIndex={1}, lastApplied={2}", logs, commitIndex, lastApplied));
      broadcastAppendEntries();
    }

    on Shutdown do {
      goto FinishedServing;
    }

    on RequestVote do (payload: RequestVoteData) {
      if (currentTerm < payload.term) {
        printLog(format("Saw higher term as a leader: currentTerm={0}, payload.term={1}", currentTerm, payload.term));
        handleRequestVote(payload);
        becomeFollower(payload.term);
      }
      else {
        printLog(format("Reject proposal from {0} with term {1}", payload.candidate, payload.term));
        send payload.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
      }
    }

    on RequestVoteReply do (payload: RequestVoteReplyData) {
      if (payload.term > currentTerm) {
        printLog(format("received RequestVoteReply with higher term {0}", payload.term));
        updateTermAndVote(payload.term);
        goto Follower;
      }
    }

    on HeartbeatTimeout do {
      broadcastAppendEntries();
      leaderCommits();
      notifyViewLog();
    }

    on AppendEntries do (payload: AppendEntriesData) {
      if (payload.term > currentTerm) {
        // if the leader receives an AppendEntries from another leader with a higher term
        // step down to a follower
        printLog(format("received AppendEntries with higher term {0}; handle then step down", payload.term));
        updateTermAndVote(payload.term);
        handleAppendEntries(payload);
        becomeFollower(payload.term);
      }
      else if (payload.term < currentTerm) {
        send payload.leader, AppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=-1);
        leaderCommits();
      }
    }

    on AppendEntriesReply do (payload: AppendEntriesReplyData) {
      printLog(format("Leader({0}, {1}) received AppendEntriesReply from {2}: {3}", this, currentTerm, payload.sender, payload));
      if (payload.term < currentTerm) {
        printLog("Ignore AppendEntriesReply with outdated term");
        return;
      }

      if (payload.term > currentTerm) {
        // if the leader receives a reply from another leader with a higher term
        // step down to a follower
        printLog(format("received reply from another leader with term {0} > currentTerm {1}", payload.term, currentTerm));
        updateTermAndVote(payload.term);
        goto Follower;
      }

      if (payload.success) {
        // if the heartbeat is accepted,
        // update the nextIndex and matchIndex for the corresponding node.
        printLog(format("entries accepted by {0}; matchIndex={1}", payload.sender, payload.matchedIndex));
        // Note that the leader can receive delayed message, so here we need to keep the
        // larger values of nextIndex and matchIndex.
        nextIndex[payload.sender] = Max(nextIndex[payload.sender], payload.matchedIndex + 1);
        matchIndex[payload.sender] = Max(matchIndex[payload.sender], payload.matchedIndex);
        leaderCommits();
      }
      else {
        if (payload.matchedIndex < 0) {
          // a rejection of me as a leader in an earlier term
          // this can happen when me and the other node simultaneously become a candidate
          // the other node received my heartbeat from the previous term and sent me a rejection
          // but currently, I am already at the newer term, so I can safely ignore this rejection
          printLog(format("outdated rejection with term={0}, matchedIndex={1}", payload.term, payload.matchedIndex));
          return;
        }

        // rejected because of log mismatch
        // now, payload.matchedIndex is the index of the (potentially) first mismatched term
        printLog(format("re-sync with {0} at index {1}", payload.sender, payload.matchedIndex));
        // still, the leader can receive outdated message. To be conservative, it is safe to synchronize from
        // a smaller index.
        nextIndex[payload.sender] = Min(payload.matchedIndex, nextIndex[payload.sender]);
      }
    }

    on ClientGetRequest do (payload: ClientGetRequestData) {
      var executionResult: ExecutionResult;
      if (checkClientRequestCache(payload.client, payload.transactionId)) {
        return;
      }

      executionResult = executeGet(kvStore, payload.key);
      send payload.client, RaftGetResponse, (sender=this, client=payload.client, success=executionResult.result.success, transactionId=payload.transactionId, value=executionResult.result.value);
      if (!(payload.client in keys(clientGetRequestCache))) {
        clientGetRequestCache[payload.client] = default(map[int, RaftGetResponseData]);
      }

      clientGetRequestCache[payload.client][payload.transactionId] = (sender=this, client=payload.client, success=executionResult.result.success, transactionId=payload.transactionId, value=executionResult.result.value);
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      var newEntry: ServerLog;
      var target: Server;
      var entries: seq[ServerLog];
      var i: int;
      // print format("Received client request {0}", payload);
      if (checkClientRequestCache(payload.client, payload.transactionId)) {
        return;
      }

      newEntry = (term=currentTerm, key=payload.key, value=payload.value, client=payload.client, transactionId=payload.transactionId);
      if (!(newEntry in logs)) {
        logs += (sizeof(logs), newEntry);
      }

      printLog(format("Current leader logs={0}", logs));
      printLog(format("nextIndex={0}, matchIndex={1}", nextIndex, matchIndex));
      // use info of nextIndex and matchIndex to broadcast to peers
      foreach (target in peers) {
        if (target != this && nextIndex[target] < sizeof(logs)) {
          entries = default(seq[ServerLog]);
          i = nextIndex[target];
          while (i < sizeof(logs)) {
            entries += (i - nextIndex[target], logs[i]);
            i = i + 1;
          }

          send target, AppendEntries, (term=currentTerm,
          leader=this,
          prevLogIndex=nextIndex[target] - 1,
          prevLogTerm=getLogTerm(logs, nextIndex[target] - 1),
          entries=entries,
          leaderCommit=commitIndex);
        }
      }

      leaderCommits();
    }

    on Reset do {
      reset();
    }

    ignore ElectionTimeout;
  }

  state FinishedServing {
    entry {
      printLog("Service ended. Shutdown.");
    }

    ignore Shutdown, RequestVote, RequestVoteReply, AppendEntries, AppendEntriesReply, ClientPutRequest, ClientGetRequest, HeartbeatTimeout, ElectionTimeout;
  }

  fun drainRequestQueuesTo(target: Server) {
    while (sizeof(clientPutRequestQueue) > 0 || sizeof(clientGetRequestQueue) > 0) {
      if (sizeof(clientPutRequestQueue) == 0) {
        send target, ClientGetRequest, forwardedGetRequest(clientGetRequestQueue[0].payload);
        clientGetRequestQueue -= (0);
      }
      else if (sizeof(clientGetRequestQueue) == 0) {
        send target, ClientPutRequest, forwardedPutRequest(clientPutRequestQueue[0].payload);
        clientPutRequestQueue -= (0);
      }
      else {
        if (clientPutRequestQueue[0].sequenceNumber < clientGetRequestQueue[0].sequenceNumber) {
          send target, ClientPutRequest, forwardedPutRequest(clientPutRequestQueue[0].payload);
          clientPutRequestQueue -= (0);
        }
        else {
          send target, ClientGetRequest, forwardedGetRequest(clientGetRequestQueue[0].payload);
          clientGetRequestQueue -= (0);
        }
      }
    }
  }

  fun respondOrForwardPut(payload: ClientPutRequestData) {
    if (checkClientRequestCache(payload.client, payload.transactionId)) {
      return;
    }

    if (leader != default(Server)) {
      send leader, ClientPutRequest, forwardedPutRequest(payload);
    }
    else {
      clientPutRequestQueue += (sizeof(clientPutRequestQueue), (sequenceNumber=requestSequenceNumber, payload=payload));
      requestSequenceNumber = requestSequenceNumber + 1;
    }
  }

  fun respondOrForwardGet(payload: ClientGetRequestData) {
    if (checkClientRequestCache(payload.client, payload.transactionId)) {
      return;
    }

    if (leader != default(Server)) {
      send leader, ClientGetRequest, forwardedGetRequest(payload);
    }
    else {
      clientGetRequestQueue += (sizeof(clientGetRequestQueue), (sequenceNumber=requestSequenceNumber, payload=payload));
      requestSequenceNumber = requestSequenceNumber + 1;
    }
  }

  fun forwardedPutRequest(request: ClientPutRequestData): ClientPutRequestData {
    return (transactionId=request.transactionId, client=request.client, key=request.key, value=request.value, sender=this);
  }

  fun forwardedGetRequest(request: ClientGetRequestData): ClientGetRequestData {
    return (transactionId=request.transactionId, client=request.client, key=request.key, sender=this);
  }

  fun handleRequestVote(reply: RequestVoteData) {
    // A handler for RequestVote messages
    leader = default(Server);
    if (reply.term < currentTerm) {
      // reject a vote with a smaller term
      printLog(format("Reject vote with term {0} < currentTerm {1}", reply.term, currentTerm));
      send reply.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
    }
    else {
      updateTermAndVote(reply.term);
      // if votedFor is null or the candidate it has voted for
      // and the log is up-to-date, grant the vote. Reject otherwise.
      if ((votedFor == default(Server) || votedFor == reply.candidate)
      && logUpToDateCheck(reply.lastLogIndex, reply.lastLogTerm)) {
        printLog(format("Grant vote to {0} (prev. votedFor={1})", reply.candidate, votedFor));
        votedFor = reply.candidate;
        send reply.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=true);
      }
      else {
        printLog(format("Reject vote with votedFor={0} and logUpToDate={1}", votedFor, logUpToDateCheck(reply.lastLogIndex, reply.lastLogTerm)));
        send reply.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
      }
    }
  }

  fun handleAppendEntries(response: AppendEntriesData) {
    // handler of leader heartbeat, log synchronization
    var myLastIndex: int;
    var myLastTerm: int;
    var ptr: int;
    var mismatchedTerm: int;
    var i: int;
    var j: int;
    printLog(format("Received AppendEntries({0}) from Leader({1}) with {2}; currentTerm={3}, log={4}", response.entries, response.leader, response, currentTerm, logs));
    if (response.term < currentTerm) {
      // if the heartbeat is from a leader with an outdated term, reject the heartbeat
      printLog(format("Reject AppendEntries with term {0} < currentTerm {1}", response.term, currentTerm));
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm,
      success=false, matchedIndex=-1);
    }
    else if (sizeof(logs) <= response.prevLogIndex) {
      // If the log is very outdated, re-sync from the end of my log
      leader = response.leader;
      printLog(format("prevLogIndex={0} is out of range of sizeof(log)={1}", response.prevLogIndex, sizeof(logs)));
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=sizeof(logs));
    }
    else if (response.prevLogIndex >= 0 && logs[response.prevLogIndex].term != response.prevLogTerm) {
      // Log consistency check failed here;
      // search for the first occurence of the mismatched
      // term and notify the leader.
      printLog(format("Log inconsistency at index {0}: log[i].term={1} v.s. prevTerm={2}", response.prevLogIndex, logs[response.prevLogIndex].term, response.prevLogTerm));
      leader = response.leader;
      ptr = response.prevLogIndex;
      mismatchedTerm = logs[ptr].term;
      // find the first occurrence of the mismatched term
      // this includes the optimization described in the paper
      while (ptr > 0 && logs[ptr].term == mismatchedTerm) {
        ptr = ptr - 1;
      }

      printLog(format("requesting sync. log terms={0}, mismatchedTerm={1}, prevLogIndex={2}, ptr={3}", termsOfLog(), mismatchedTerm, response.prevLogIndex, ptr));
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm,
      success=false, matchedIndex=ptr);
    }
    else {
      // Check if any entries disagree; delete all entries after it.
      printLog(format("Sync log with log={0} at index {1}. Entries={2}", logs, response.prevLogIndex + 1, response.entries));
      leader = response.leader;
      i = response.prevLogIndex + 1;
      j = 0;
      printLog(format("Before sync: logs={0}", logs));
      // find the first mismatched entry
      while (i < sizeof(logs) && j < sizeof(response.entries)) {
        if (logs[i].term != response.entries[j].term) {
          break;
        }

        i = i + 1;
        j = j + 1;
      }

      // delete all entries after the mismatched entry
      while (i < sizeof(logs)) {
        logs -= (lastLogIndex(logs));
      }

      printLog(format("Inconsistency removal: logs={0}", logs));
      // Append entries from the leader
      j = 0;
      while (j < sizeof(response.entries)) {
        if (!(response.entries[j] in logs)) {
          logs += (response.prevLogIndex + 1 + j, response.entries[j]);
        }
        else {
          assert response.entries[j] == logs[response.prevLogIndex + 1 + j], "Inconsistent log entry";
        }

        j = j + 1;
      }

      printLog(format("After sync: logs={0}, leaderCommit={1}, commitIndex={2}", logs, response.leaderCommit, commitIndex));
      if (response.leaderCommit > commitIndex) {
        if (response.leaderCommit > lastLogIndex(logs)) {
          commitIndex = lastLogIndex(logs);
        }
        else {
          commitIndex = response.leaderCommit;
        }
      }

      executeCommands();
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm, success=true, matchedIndex=lastLogIndex(logs));
    }
  }

  fun broadcastAppendEntries() {
    // broadcasting of leader heartbeats
    var target: Server;
    var i: int;
    var j: int;
    var prevIndex: int;
    var prevTerm: int;
    var entries: seq[ServerLog];
    assert this == leader, "Only leader can broadcast AppendEntries";
    // print format("Broadcasting AppendEntries with log {0}", logs);
    foreach (target in peers) {
      if (this != target) {
        entries = default(seq[ServerLog]);
        prevIndex = nextIndex[target] - 1;
        // get prevIndex and prevIndexTerm
        if (prevIndex >= 0 && prevIndex < sizeof(logs)) {
          prevTerm = logs[prevIndex].term;
        }
        else {
          prevTerm = 0;
        }

        if (nextIndex[target] < sizeof(logs)) {
          // if the current lead has something to send (nextIndex within range of logs)
          i = 0;
          j = nextIndex[target];
          // aggregate the logs to be synchronized
          while (j < sizeof(logs)) {
            entries += (i, logs[j]);
            i = i + 1;
            j = j + 1;
          }

          printLog(format("Entries for {0} is {1}", target, entries));
          send target, AppendEntries, (term=currentTerm,
          leader=this,
          prevLogIndex=prevIndex,
          prevLogTerm=prevTerm,
          entries=entries,
          leaderCommit=commitIndex);
        }
        else {
          // send empty heartbeat
          send target, AppendEntries, (term=currentTerm,
          leader=this,
          prevLogIndex=prevIndex,
          prevLogTerm=prevTerm,
          entries=entries,
          leaderCommit=commitIndex);
        }
      }
    }
  }

  fun leaderCommits() {
    // leader committing log entries
    var executionResult: ExecutionResult;
    // the next commit index
    var nextCommit: int;
    // the number of match indices that are greater than or equal to nextCommit
    var validMatchIndices: int;
    var i: int;
    var target: Server;
    assert this == leader, "Only leader can execute the log on its own";
    // iteratively search for that index
    nextCommit = lastLogIndex(logs);
    printLog(format("state: nextCommit={0} commitIndex={1} matchIndex={2}", nextCommit, commitIndex, matchIndex));
    // Find the largest nextCommit such that the majority of matchIndex is greater than or equal to nextCommit
    while (nextCommit > commitIndex) {
      // counting itself first
      validMatchIndices = 1;
      foreach (target in peers) {
        if (target == this) {
          continue;
        }

        if (matchIndex[target] >= nextCommit) {
          validMatchIndices = validMatchIndices + 1;
        }
      }

      // if the majority of matchIndex is greater than or equal to nextCommit
      // also the term is currentTerm in this entry
      if (validMatchIndices > clusterSize / 2 && logs[nextCommit].term == currentTerm) {
        commitIndex = nextCommit;
        break;
      }

      nextCommit = nextCommit - 1;
    }

    // commitIndex = nextCommit;
    printLog(format("leader commits decision: matchIndex={0} commitIndex={1} currentTerm={2} logs={3}", matchIndex, commitIndex, currentTerm, logs));
    // commit all the logs from lastApplied + 1 to commitIndex
    while (lastApplied < commitIndex) {
      lastApplied = lastApplied + 1;
      announce EntryApplied, (logIndex=lastApplied, term=logs[lastApplied].term, key=logs[lastApplied].key, value=logs[lastApplied].value, transactionId=logs[lastApplied].transactionId, client=logs[lastApplied].client);
      executionResult = executePut(kvStore, logs[lastApplied].key, logs[lastApplied].value);
      kvStore = executionResult.newState;
      printLog(format("leader committed and processed (by {0}), log: {1}", this, logs[lastApplied]));
      // execute the command and send the result back to the client
      send logs[lastApplied].client, RaftPutResponse, (sender=this, client=logs[lastApplied].client,
      transactionId=logs[lastApplied].transactionId, key=logs[lastApplied].key);
      // update the cache
      if (!(logs[lastApplied].client in keys(clientPutRequestCache))) {
        clientPutRequestCache[logs[lastApplied].client] = default(map[int, RaftPutResponseData]);
      }

      clientPutRequestCache[logs[lastApplied].client][logs[lastApplied].transactionId] = (sender=this, client=logs[lastApplied].client, transactionId=logs[lastApplied].transactionId, key=logs[lastApplied].key);
    }
  }

  fun checkClientRequestCache(client: Client, transactionId: TransactionId): bool {
    // if the result is already in the cache, send it back
    if (client in clientGetRequestCache && transactionId in keys(clientGetRequestCache[client])) {
      send client, RaftGetResponse, clientGetRequestCache[client][transactionId];
      return true;
    }

    if (client in clientPutRequestCache && transactionId in keys(clientPutRequestCache[client])) {
      send client, RaftPutResponse, clientPutRequestCache[client][transactionId];
      return true;
    }

    return false;
  }

  fun printLog(msg: string) {
    // return;
    // print format("{0}@{1}: {2}", this, currentTerm, msg);
  }

  fun executeCommands() {
    var executionResult: ExecutionResult;
    printLog(format("commitIndex={0} lastApplied={1}", commitIndex, lastApplied));
    // Execute the command from lastApplied + 1 to commitIndex
    while (lastApplied < commitIndex) {
      lastApplied = lastApplied + 1;
      announce EntryApplied, (logIndex=lastApplied, term=logs[lastApplied].term, key=logs[lastApplied].key, value=logs[lastApplied].value, transactionId=logs[lastApplied].transactionId, client=logs[lastApplied].client);
      executionResult = executePut(kvStore, logs[lastApplied].key, logs[lastApplied].value);
      kvStore = executionResult.newState;
      // clientRequestCache[logs[lastApplied].client][logs[lastApplied].transactionId] = (client=logs[lastApplied].client, transactionId=logs[lastApplied].transactionId, result=executionResult.result);
    }
  }

  fun logUpToDateCheck(lastIndex: LogIndex, lastTerm: TermId): bool {
    // Given `lastIndex` and `lastTerm` from the other node,
    // check if its logs is up-to-date.
    if (lastTerm > lastLogTerm(logs)) {
      // the lastTerm is greater than mine, then it is up-to-date
      return true;
    }

    if (lastTerm < lastLogTerm(logs)) {
      // the lastTerm is smaller than mine, then it is not up-to-date
      return false;
    }

    // the lastTerm is the same, then whichever's log is longer is up-to-date
    return lastIndex >= lastLogIndex(logs);
  }

  fun notifyViewLog() {
    logNotifyTimestamp = logNotifyTimestamp + 1;
    send viewServer, NotifyLog, (timestamp=logNotifyTimestamp, server=this, log=logs);
  }

  fun becomeFollower(term: int) {
    // cancelTimer(electionTimer);
    updateTermAndVote(term);
    goto Follower;
  }

  fun updateTermAndVote(term: int) {
    if (term > currentTerm) {
      // if the term is changed, then reset the votedFor since
      // it has not voted in the new term.
      votedFor = default(Server);
    }

    // the only case is currentTerm == term
    currentTerm = term;
  }

  fun termsOfLog(): seq[int] {
    var terms: seq[int];
    var i: int;
    terms = default(seq[int]);
    i = 0;
    while (i < sizeof(logs)) {
      terms += (sizeof(terms), logs[i].term);
      i = i + 1;
    }

    return terms;
  }

  fun reset() {
    // crash a node, reset volatile states
    // and become a follower (just as it starts up)
    commitIndex = -1;
    lastApplied = -1;
    nextIndex = fillMap(this, nextIndex, peers, 0);
    matchIndex = fillMap(this, matchIndex, peers, -1);
    goto Follower;
  }
}

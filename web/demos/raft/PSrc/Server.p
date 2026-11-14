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

// A candidate's reminder to itself to ask its next peer for a vote
event AskForVote;

// events to notify the server role change to the view server
event ViewChangedFollower: Server;
event ViewChangedLeader: Server;
event ViewChangedCandidate: Server;

// server logs
// Puts and gets both go through the log, so a get is answered only once it commits.
type ServerLog = (term: TermId, op: Op, key: KeyT, value: ValueT, client: Client, transactionId: TransactionId);

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
  // peers a candidate has not yet asked for a vote this term
  var unasked: set[Server];

  // all servers
  var commitIndex: int;
  var lastApplied: int;

  // var heartbeatTimer: Timer;
  // var clientRequestQueue: seq[ClientRequestData];
  var requestSequenceNumber: int;
  var clientPutRequestQueue: seq[(sequenceNumber: int, payload: ClientPutRequestData)];
  var clientGetRequestQueue: seq[(sequenceNumber: int, payload: ClientGetRequestData)];
  var clientPutRequestCache: map[Client, map[int, RaftPutResponseData]];
  var clientGetRequestCache: map[Client, map[int, RaftGetResponseData]];
  // Part of the state machine: each client's last applied transaction and its result.
  // A retried request can reach the log twice (a new leader appends it again in its own
  // term); the second copy must not run again (Raft paper, section 8).
  var sessions: map[Client, (transactionId: TransactionId, result: Result)];

  start state Init {
    entry {
    }

    // every other variable starts at its default: term 0, empty log, no leader, no vote
    on ServerInit do (setup: (myId: ServerId, cluster: set[Server], viewServer: View)) {
      assert this in setup.cluster, "Server should be in the cluster";
      clusterSize = sizeof(setup.cluster);
      peers = setup.cluster;
      viewServer = setup.viewServer;
      commitIndex = -1;
      lastApplied = -1;
      goto Follower;
    }

    ignore Reset, ElectionTimeout, HeartbeatTimeout;
  }

  state Follower {
    entry {
      // reset the leader upon being a follower
      leader = default(Server);
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
      goto Candidate;
    }

    on HeartbeatTimeout do {
      notifyViewLog();
    }

    // a reminder left over from an earlier candidacy
    ignore RequestVoteReply, AppendEntriesReply, AskForVote;
  }

  state Candidate {
    entry {
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
        // Ask the peers one at a time, in an order the checker picks. Requests to
        // different peers travel separately, so one peer can answer, or hear of a newer
        // term, before another is asked.
        unasked = peers;
        unasked -= (this);
        send this, AskForVote;
      }
    }

    // the reminder after the last peer, or one left from an earlier term, finds no one
    on AskForVote do {
      var peer: Server;
      if (sizeof(unasked) > 0) {
        peer = choose(unasked);
        unasked -= (peer);
        send peer, RequestVote, (term=currentTerm, candidate=this, lastLogIndex=lastLogIndex(logs), lastLogTerm=lastLogTerm(logs));
        send this, AskForVote;
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
        updateTermAndVote(payload.term);
        handleAppendEntries(payload);
        goto Follower;
      }
      else {
        handleAppendEntries(payload);
      }
    }

    on AppendEntriesReply do (payload: AppendEntriesReplyData) {
      if (payload.term > currentTerm) {
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
        if (sizeof(votesReceived) > clusterSize / 2) {
          // if majority votes are received, become a leader
          goto Leader;
        }
      }

      // otherwise a rejection or a reply from an earlier term: nothing to count
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
      broadcastAppendEntries();
    }

    on Shutdown do {
      goto FinishedServing;
    }

    on RequestVote do (payload: RequestVoteData) {
      if (currentTerm < payload.term) {
        handleRequestVote(payload);
        becomeFollower(payload.term);
      }
      else {
        send payload.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
      }
    }

    on RequestVoteReply do (payload: RequestVoteReplyData) {
      if (payload.term > currentTerm) {
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
      // a newer leader won without asking this server for its vote: step down
      // snappea-lint: allow mutation:swap-operator -- `>=` is equivalent: an equal term would mean two leaders in one term, which SafetyOneLeader rules out
      if (payload.term > currentTerm) {
        updateTermAndVote(payload.term);
        handleAppendEntries(payload);
        becomeFollower(payload.term);
      }

      // payload.term < currentTerm: two leaders never share a term (SafetyOneLeader)
      send payload.leader, AppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=-1);
      leaderCommits();
    }

    on AppendEntriesReply do (payload: AppendEntriesReplyData) {
      if (payload.term < currentTerm) {
        return;
      }

      if (payload.term > currentTerm) {
        // if the leader receives a reply from another leader with a higher term
        // step down to a follower
        updateTermAndVote(payload.term);
        goto Follower;
      }

      if (payload.success) {
        // if the heartbeat is accepted,
        // update the nextIndex and matchIndex for the corresponding node.
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
          return;
        }

        // rejected because of log mismatch
        // now, payload.matchedIndex is the index of the (potentially) first mismatched term
        // still, the leader can receive outdated message. To be conservative, it is safe to synchronize from
        // a smaller index.
        nextIndex[payload.sender] = Min(payload.matchedIndex, nextIndex[payload.sender]);
      }
    }

    // A get goes through the log like a put. Answering from the local store would let a
    // new leader that has not yet applied every committed put return an older value.
    on ClientGetRequest do (payload: ClientGetRequestData) {
      if (checkClientRequestCache(payload.client, payload.transactionId)) {
        return;
      }

      appendAndReplicate((term=currentTerm, op=GET, key=payload.key, value=-1, client=payload.client, transactionId=payload.transactionId));
    }

    on ClientPutRequest do (payload: ClientPutRequestData) {
      if (checkClientRequestCache(payload.client, payload.transactionId)) {
        return;
      }

      appendAndReplicate((term=currentTerm, op=PUT, key=payload.key, value=payload.value, client=payload.client, transactionId=payload.transactionId));
    }

    on Reset do {
      reset();
    }

    // a reminder left over from the candidacy that won
    ignore ElectionTimeout, AskForVote;
  }

  state FinishedServing {
    entry {
    }

    ignore Shutdown, RequestVote, RequestVoteReply, AppendEntries, AppendEntriesReply, ClientPutRequest, ClientGetRequest, HeartbeatTimeout, ElectionTimeout, AskForVote;
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
        // snappea-lint: allow mutation:swap-operator -- `<=` is equivalent: each queued request gets its own sequence number, so they are never equal
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
      send reply.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=false);
    }
    else {
      updateTermAndVote(reply.term);
      // if votedFor is null or the candidate it has voted for
      // and the log is up-to-date, grant the vote. Reject otherwise.
      if ((votedFor == default(Server) || votedFor == reply.candidate)
      && logUpToDateCheck(reply.lastLogIndex, reply.lastLogTerm)) {
        votedFor = reply.candidate;
        send reply.candidate, RequestVoteReply, (sender=this, term=currentTerm, voteGranted=true);
      }
      else {
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
    if (response.term < currentTerm) {
      // if the heartbeat is from a leader with an outdated term, reject the heartbeat
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm,
      success=false, matchedIndex=-1);
    }
    else if (sizeof(logs) <= response.prevLogIndex) {
      // If the log is very outdated, re-sync from the end of my log
      leader = response.leader;
      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm, success=false, matchedIndex=sizeof(logs));
    }
    else if (response.prevLogIndex >= 0 && logs[response.prevLogIndex].term != response.prevLogTerm) {
      // Log consistency check failed here;
      // search for the first occurence of the mismatched
      // term and notify the leader.
      leader = response.leader;
      ptr = response.prevLogIndex;
      mismatchedTerm = logs[ptr].term;
      // find the first occurrence of the mismatched term
      // this includes the optimization described in the paper
      while (ptr > 0 && logs[ptr].term == mismatchedTerm) {
        ptr = ptr - 1;
      }

      send response.leader, AppendEntriesReply, (sender=this, term=currentTerm,
      success=false, matchedIndex=ptr);
    }
    else {
      // Check if any entries disagree; delete all entries after it.
      leader = response.leader;
      i = response.prevLogIndex + 1;
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

      if (response.leaderCommit > commitIndex) {
        commitIndex = Min(response.leaderCommit, lastLogIndex(logs));
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
        prevTerm = getLogTerm(logs, prevIndex);

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

  fun appendAndReplicate(newEntry: ServerLog) {
    var target: Server;
    var entries: seq[ServerLog];
    var i: int;
    if (!(newEntry in logs)) {
      logs += (sizeof(logs), newEntry);
    }

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

  fun leaderCommits() {
    // leader committing log entries
    // the next commit index
    var nextCommit: int;
    // the number of match indices that are greater than or equal to nextCommit
    var validMatchIndices: int;
    var i: int;
    var target: Server;
    assert this == leader, "Only leader can execute the log on its own";
    // iteratively search for that index
    nextCommit = lastLogIndex(logs);
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
    // apply the logs from lastApplied + 1 to commitIndex and answer their clients
    while (lastApplied < commitIndex) {
      lastApplied = lastApplied + 1;
      replyToClient(logs[lastApplied], applyEntry(lastApplied));
    }
  }

  // Apply one committed entry to the store and return its result.
  fun applyEntry(index: LogIndex): Result {
    var executionResult: ExecutionResult;
    var client: Client;
    announce EntryApplied, (logIndex=index, log=logs[index]);
    client = logs[index].client;
    if (client in sessions && logs[index].transactionId <= sessions[client].transactionId) {
      // a copy of a request that already ran
      return sessions[client].result;
    }

    if (logs[index].op == PUT) {
      executionResult = executePut(kvStore, logs[index].key, logs[index].value);
    }
    else {
      executionResult = executeGet(kvStore, logs[index].key);
    }

    kvStore = executionResult.newState;
    sessions[client] = (transactionId=logs[index].transactionId, result=executionResult.result);
    return executionResult.result;
  }

  // The leader answers the client of an applied entry and caches the answer for retries.
  fun replyToClient(applied: ServerLog, result: Result) {
    var putResponse: RaftPutResponseData;
    var getResponse: RaftGetResponseData;
    if (applied.op == PUT) {
      putResponse = (sender=this, client=applied.client, transactionId=applied.transactionId, key=applied.key);
      send applied.client, RaftPutResponse, putResponse;
      if (!(applied.client in keys(clientPutRequestCache))) {
        clientPutRequestCache[applied.client] = default(map[int, RaftPutResponseData]);
      }

      clientPutRequestCache[applied.client][applied.transactionId] = putResponse;
    }
    else {
      getResponse = (sender=this, client=applied.client, success=result.success, transactionId=applied.transactionId, value=result.value);
      send applied.client, RaftGetResponse, getResponse;
      if (!(applied.client in keys(clientGetRequestCache))) {
        clientGetRequestCache[applied.client] = default(map[int, RaftGetResponseData]);
      }

      clientGetRequestCache[applied.client][applied.transactionId] = getResponse;
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

  fun executeCommands() {
    // a follower applies the entries from lastApplied + 1 to commitIndex; the leader answers
    while (lastApplied < commitIndex) {
      lastApplied = lastApplied + 1;
      applyEntry(lastApplied);
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
    send viewServer, NotifyLog, (server=this, log=logs);
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

  fun reset() {
    // crash a node, reset volatile states
    // and become a follower (just as it starts up); the log and term persist, and the
    // store is rebuilt by applying the log again
    commitIndex = -1;
    lastApplied = -1;
    kvStore = newStore();
    sessions = default(map[Client, (transactionId: TransactionId, result: Result)]);
    nextIndex = fillMap(this, nextIndex, peers, 0);
    matchIndex = fillMap(this, matchIndex, peers, -1);
    goto Follower;
  }
}

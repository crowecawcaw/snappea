// Modules name groups of machines, so test cases can be composed from them.
module Allocation = { Allocator };
module Callers = { Caller };
// A binding: wherever the model says `new Allocator(...)`, create a WrappingAllocator.
module WrappingAllocation = { WrappingAllocator -> Allocator };

fun Start(limit: int, callers: int, requests: int) {
  var allocator: Allocator;
  var i: int;
  announce AllocatorCreated, limit;
  allocator = new Allocator(limit);
  while (i < callers) {
    new Caller((allocator = allocator, requests = requests));
    i = i + 1;
  }
}

// Contention: two callers at once, fewer requests than ids.
machine TwoCallers {
  start state Init {
    entry { Start(3, 2, 1); }
  }
}

// Boundary: one caller asks for one more id than there are.
machine OneMoreThanLimit {
  start state Init {
    entry { Start(3, 1, 4); }
  }
}

test tcTwoCallers [main=TwoCallers]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { TwoCallers });

// snappea: boundary -- exhaust the id range and request one more
test tcOneMoreThanLimit [main=OneMoreThanLimit]:
  assert UniqueIds, RequestsAnswered in (union Allocation, Callers, { OneMoreThanLimit });

// Same driver, broken allocator, and only the spec this bug is about. The progress
// spec is checked by the passing tests; this negative control is about duplicates.
// snappea: expect-bug "id 0 given twice"
test tcBugWrapsAround [main=OneMoreThanLimit]:
  assert UniqueIds in (union WrappingAllocation, Callers, { OneMoreThanLimit });

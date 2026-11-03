import { Island } from "../Island";
import { links } from "../SiteHeader";
import { Code } from "./Code";
import { Trace } from "./diagrams/Trace";

export function FirstModel({ root }: { root: string }) {
  const DEMO = `${links(root).playground}?demo=lost-update`;
  return <>
    <section className="hero">
      <p className="eyebrow">Learn</p><h1>Your first model</h1>
      <p className="lede">Find a lost update, inspect its trace, and change the design.</p>
      <p>No installation or P experience is needed. You will use the browser Playground.</p>
    </section>
    <section>
      <h2>Run the broken design</h2>
      <p><a href={DEMO}>Open the lost-update model</a>. Press Check to run its test.
        Two clients each increment a counter. The assertion expects 2, but an execution can leave 1.
        Read the failing assertion and inspect the saved trace before changing the code.</p>
    </section>
      <section>
        <h2>Read the model</h2>
        <p>
          Each <b>machine</b> is a state machine with an inbox. <code>send</code> puts an event in another machine's
          inbox, and the checker decides when it's delivered. Here the Counter serves reads and writes, and each
          Client reads, then writes back the value plus one:
        </p>
        <Code lang="p" title="LostUpdate.p">{`
machine Counter {
var value: int;
start state Serving {
  on ReadValue do (client: Client) {
    send client, ValueRead, value;
  }
  on WritValueRead do (v: int) { value = v; }
  on ReadFinalValue do (testHarness: Main) {
    send testHarness, ValueRead, value;
  }
}
}

machine Client {
start state Run {
  entry (input: (counter: Counter, owner: Main)) {
    var rememberedValue: int;
    send input.counter, ReadValue, this;
    receive {
      case ValueRead: (replyValue: int) {
        rememberedValue = replyValue;
      }
    }
    send input.counter, WritValueRead, v + 1;
    send input.owner, ClientFinished;
  }
}
}
`}</Code>
        <p>
          The test machine, <code>Main</code>, starts the Counter and two Clients. Once both Clients are done, it
          asserts that the counter reached 2:
        </p>
        <Code lang="p">{`
assert finalValue == 2, format("expected 2 increments, counter is {0}", finalValue);

test tcTwoClients [main=Main]: { Main, Counter, Client };
`}</Code>
        <p>If you have installed the CLI and saved the Playground file as <code>LostUpdate.p</code>, the equivalent check looks like this (timings and schedule counts vary):</p>
        <Code lang="shell">{`
$ snappea check LostUpdate.p tcTwoClients
Checking generatedOutput: 1 test case
FAIL  tcTwoClients (bug found after 9 schedules, 8 workers, 0.00s)
      Assertion Failed: LostUpdate.p:69:7 expected 2 increments, counter is 1
      Replay: snappea replay PCheckerOutput/tcTwoClients/bug.schedule LostUpdate.p
1 of 1 test case failed in 0.01s, peak memory 9 MB. Output: PCheckerOutput/
`}</Code>
        <p>The trace shows exactly how the update was lost:</p>
        <Island name="trace"><Trace /></Island>
        <p>
          <b>The fix is a design change:</b> replace the read and write with one <code>Increment</code> event that
          the Counter applies itself. For this small model, automatic exhaustive exploration can finish and find no bug. Read the result to confirm whether it completed.
        </p>
        <Code lang="p">{`
on Increment do { value = value + 1; }
`}</Code>
        <p>
          <a className="button primary" href={DEMO}>Open this model in the Playground →</a>
        </p>
      </section>


    <section>
      <h2>Check the fix</h2>
      <p>In the Playground, change the Counter to handle one increment event. Change each Client to send that event
        instead of reading and writing. Keep the final assertion: two completed increments must leave 2.
        Declare the new event and remove unused read/write declarations. Run the checker again.</p>
      <Code lang="p" title="Replacement event and handlers (excerpts)">{`
event Increment;

// Counter.Serving: replace on ReadValue and on WritValueRead with:
on Increment do { value = value + 1; }

// Client.Run: replace its entry with:
entry (input: (counter: Counter, owner: Main)) {
  send input.counter, Increment;
  send input.owner, ClientFinished;
}
`}</Code>
      <p>Keep <code>ValueRead</code> and the Counter's <code>ReadFinalValue</code> handler: Main still uses them to verify the final value.</p>
      <p>The fixed design should pass. Read the result to see whether exploration completed or reached its budget.
        A pass checks this bounded model, not every possible implementation.</p>
      <p>You have changed the design, not just the test order. Next, use the
        <a href="./agents.html"> agent workflow</a> to carry a model's rules into application tests.</p>
    </section>
  </>;
}

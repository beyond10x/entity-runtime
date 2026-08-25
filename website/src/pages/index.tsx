import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

const holds = [
  {
    title: 'The kernel does no IO',
    text: 'No clock, no identifier generator, no filesystem, no network, no randomness, no async. A source scan and a pinned dependency list keep it that way.',
    pin: 'tests/purity.rs',
  },
  {
    title: 'Same inputs, same decision, same bytes',
    text: 'Ordered maps only. Time and identity arrive as arguments. A production decision replays a year later to the byte.',
    pin: 'R-02 · R-05',
  },
  {
    title: 'A refusal changes nothing',
    text: 'Instances are taken by reference and a new one is returned. A failed rule leaves no half transition and no stray event.',
    pin: 'R-04',
  },
  {
    title: 'The lifecycle is not a field',
    text: 'Nothing writes the state except create and execute. A status change is a named operation with arguments and rules, or it does not happen.',
    pin: 'R-34',
  },
  {
    title: 'Rules see only their scope',
    text: 'A precondition may read the arguments; an invariant may not. The wrong reference is refused when the definition is registered, not at run time.',
    pin: 'R-52 · R-14',
  },
  {
    title: 'Every requirement is pinned',
    text: 'Fifty rows, each naming the test, type or manifest that holds it. The gate fails when a cited test stops existing.',
    pin: 'check-requirements.py',
  },
];

const steps = [
  ['01', 'Identity', 'The instance matches the definition'],
  ['02', 'Operation', 'The definition declares it'],
  ['03', 'Arguments', 'Defaulted, then validated'],
  ['04', 'Transition', 'Selected from the current state'],
  ['05', 'Preconditions', 'Against state and arguments'],
  ['06', 'Set', 'Every assignment reads pre-operation fields'],
  ['07', 'Fields', 'Validated against the schema again'],
  ['08', 'Next state', 'New lifecycle state, revision + 1'],
  ['09', 'Invariants', 'Against the next state'],
  ['10', 'Events', 'Materialised from templates'],
  ['11', 'Decision', 'Returned. Nothing persisted, nothing published'],
];

const neighbours = [
  {
    number: '01',
    title: 'engineering-protocols',
    text: 'The intended first adopter. Its artifact model — kinds, lifecycles, legal moves, events — expressed as definitions this kernel executes, so a new status is a line of YAML rather than a Rust release.',
    signal: 'Proposed · phased 0–4',
    to: '/docs/design/engineering-protocols-adoption-v0.1',
  },
  {
    number: '02',
    title: 'eventlog',
    text: 'The append-only side. This crate decides and emits events; that one stores, folds and projects them. Neither pretends to be the other.',
    signal: 'Decides · stores',
    to: '/docs/design/kernel-v0.1#10-event-sourcing-without-mandating-it',
  },
  {
    number: '03',
    title: 'Your shell',
    text: 'Whatever loads the instance, calls the kernel, and persists the decision together with its events. The entity command is the reference one; a service is the next.',
    signal: 'Load · decide · persist · publish',
    to: '/docs/guide/library',
  },
];

function DecisionPanel(): ReactNode {
  return (
    <aside className={styles.panel} aria-label="A definition and a refusal">
      <div className={styles.panelTop}>
        <span>order.yaml · approve</span>
        <span className={styles.refused}>refused</span>
      </div>
      <pre className={styles.panelCode}>
        <code>{`approve:
  transitions: [{ from: submitted, to: approved }]
  preconditions:
    - name: positive_total
      assert: { gt: [$fields.total_cents, 0] }
      message: zero-value orders cannot be approved`}</code>
      </pre>
      <div className={styles.panelOut}>
        <span className={styles.panelLabel}>entity execute · exit 1</span>
        <pre>
          <code>{`{
  "kind": "precondition_failed",
  "operation": "approve",
  "rule": "positive_total",
  "reason": "zero-value orders cannot be approved"
}`}</code>
        </pre>
      </div>
      <div className={styles.boundary}>
        <span>NOTHING CHANGED</span>
        <p>The instance is still submitted at revision 2. No event left the kernel.</p>
      </div>
    </aside>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title="Entity types declared as data, decided by an IO-free kernel"
      description="A schema-driven entity runtime: schema, lifecycle, operations, rules and events declared once in YAML; a deterministic Rust kernel decides definition + instance + operation + arguments → Decision. A library and a CLI.">
      <main>
        <header className={styles.hero}>
          <div className={styles.heroGlow} />
          <div className={`container ${styles.heroGrid}`}>
            <div className={styles.heroCopy}>
              <div className={styles.eyebrow}>
                <span /> Declared once · decided every time
              </div>
              <Heading as="h1">
                A state change is an operation, <em>or it does not happen.</em>
              </Heading>
              <p className={styles.lede}>
                Declare an entity type as data — fields, states, the operations that move
                between them, the rules each demands, the events each emits. One IO-free
                kernel decides every one of them the same way.
              </p>
              <div className={styles.actions}>
                <Link className={styles.primaryAction} to="/docs/guide/getting-started">
                  Get started <span aria-hidden="true">↗</span>
                </Link>
                <Link className={styles.secondaryAction} to="/docs/design/kernel-v0.1">
                  Read the design
                </Link>
              </div>
              <div className={styles.metrics} aria-label="At a glance">
                <div>
                  <strong>3</strong>
                  <span>crates: kernel · yaml · cli</span>
                </div>
                <div>
                  <strong>13</strong>
                  <span>condition operators, no code</span>
                </div>
                <div>
                  <strong>0</strong>
                  <span>clocks, ids or sockets in the kernel</span>
                </div>
              </div>
            </div>
            <DecisionPanel />
          </div>
        </header>

        <section className={styles.thesis}>
          <div className="container">
            <p className={styles.sectionLabel}>The one rule</p>
            <Heading as="h2">
              definition + instance + operation + arguments → Decision
            </Heading>
            <div className={styles.thesisGrid}>
              <p>
                Commands never mutate state. An operation is evaluated against the current
                instance and yields a new instance and the events that describe the change.
                The kernel persists nothing and publishes nothing; the shell that called it
                decides whether to keep the result.
              </p>
              <p>
                Because the kernel reaches no clock, no filesystem and no random source, the
                same inputs give the same answer — which is what makes a definition testable
                in milliseconds and a refusal a fact with an address: which operation, which
                state, which rule.
              </p>
            </div>
          </div>
        </section>

        <section className={styles.holds}>
          <div className="container">
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.sectionLabel}>What holds</p>
                <Heading as="h2">Properties with a test behind each.</Heading>
              </div>
              <Link to="/docs/requirements">The full register →</Link>
            </div>
            <div className={styles.holdGrid}>
              {holds.map((hold) => (
                <article key={hold.title}>
                  <Heading as="h3">{hold.title}</Heading>
                  <p>{hold.text}</p>
                  <strong>{hold.pin}</strong>
                </article>
              ))}
            </div>
          </div>
        </section>

        <section className={styles.order} id="order">
          <div className="container">
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.sectionLabel}>Evaluation order</p>
                <Heading as="h2">Eleven steps. A refusal at any one returns before the next.</Heading>
              </div>
              <p className={styles.orderIntro}>
                The order is the contract: <em>you cannot do that from here</em> is never
                masked by <em>and your total is zero</em>, and the state a rule judges is the
                state that would be stored.
              </p>
            </div>
            <ol className={styles.steps}>
              {steps.map(([number, title, text]) => (
                <li key={number}>
                  <span>{number}</span>
                  <Heading as="h3">{title}</Heading>
                  <p>{text}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        <section className={styles.neighbours} id="neighbours">
          <div className="container">
            <p className={styles.sectionLabel}>Where it sits</p>
            <Heading as="h2">The kernel decides. Everything else is a neighbour.</Heading>
            <div className={styles.neighbourGrid}>
              {neighbours.map((neighbour) => (
                <article key={neighbour.number}>
                  <span className={styles.neighbourNumber}>{neighbour.number}</span>
                  <Heading as="h3">{neighbour.title}</Heading>
                  <p>{neighbour.text}</p>
                  <Link to={neighbour.to}>{neighbour.signal} →</Link>
                </article>
              ))}
            </div>
          </div>
        </section>

        <section className={styles.closing}>
          <div className={`container ${styles.closingInner}`}>
            <div>
              <p className={styles.sectionLabel}>Stated plainly</p>
              <Heading as="h2">Rules are two-valued. That is a known limit.</Heading>
            </div>
            <div className={styles.closingAction}>
              <p>
                A reference that does not resolve reads <em>false</em>, which is enough for a
                lifecycle and not enough for an evidence gate that must tell <em>nobody looked</em>{' '}
                from <em>it is wrong</em>. The three-valued extension is the first story on the
                board, and the design says why.
              </p>
              <Link className={styles.primaryAction} to="/docs/design/kernel-v0.1#4-the-condition-language">
                Read the limitation <span aria-hidden="true">↗</span>
              </Link>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

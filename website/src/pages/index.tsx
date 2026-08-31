import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

const benefits = [
  {
    label: '01',
    title: 'Policy lives outside the prompt',
    text: 'Schema, lifecycle, operations, rules, and events are validated data. The model does not reinterpret policy on every call.',
  },
  {
    label: '02',
    title: 'Agents operate, never patch',
    text: 'Expose approve, reject, or submit—not a writable status field. Every transition has declared arguments and rules.',
  },
  {
    label: '03',
    title: 'Refusals are useful data',
    text: 'A typed result names the state, rule, invalid path, or missing evidence. Nothing changes and no event escapes.',
  },
  {
    label: '04',
    title: 'Accepted work carries proof',
    text: 'Record the command, definition, result, actor, time, changes, and events. Replay reruns the decision instead of trusting it.',
  },
  {
    label: '05',
    title: 'Races do not become overwrites',
    text: 'Providers compare the revision a caller saw and commit state with its evidence. A stale writer is refused.',
  },
  {
    label: '06',
    title: 'The kernel stays predictable',
    text: 'No clock, filesystem, network, randomness, or hidden lookup. Same inputs, same decision, same bytes.',
  },
];

const flow = [
  ['01', 'Agent proposes', 'A named operation and domain arguments'],
  ['02', 'Shell establishes trust', 'Canonical state, identity, authority, and provenance'],
  ['03', 'Runtime decides', 'Transition, schema, preconditions, assignments, invariants'],
  ['04', 'System responds', 'Commit a Decision—or return a typed refusal unchanged'],
];

const fits = [
  'Approvals and evidence gates',
  'Case, claim, and ticket lifecycles',
  'Agent-operated business workflows',
  'Auditable human-and-agent actions',
  'Offline or hybrid state boundaries',
  'Deterministic tool execution',
];

function DecisionPanel(): ReactNode {
  return (
    <aside className={styles.panel} aria-label="A large refund proposed by an agent is refused">
      <div className={styles.panelTop}>
        <span>refund-104 · approve</span>
        <span className={styles.refused}>refused</span>
      </div>
      <div className={styles.proposal}>
        <span>agent proposal</span>
        <pre>
          <code>{`operation: approve
arguments:
  actor_role: agent
  reason: delivery evidence supplied`}</code>
        </pre>
      </div>
      <div className={styles.panelOut}>
        <span>deterministic result · exit 1</span>
        <pre>
          <code>{`{
  "kind": "precondition_failed",
  "rule": "large_refunds_need_a_human",
  "reason": "refunds above 5000 cents
             require a human actor"
}`}</code>
        </pre>
      </div>
      <div className={styles.boundary}>
        <strong>NO STATE CHANGED</strong>
        <p>Still submitted at revision 2. No approval event exists.</p>
      </div>
    </aside>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title="Deterministic authority for AI agents"
      description="Let agents propose operations while a deterministic, schema-driven runtime decides whether durable state may change—and records why.">
      <main>
        <header className={styles.hero}>
          <div className={styles.heroGlow} />
          <div className={`container ${styles.heroGrid}`}>
            <div className={styles.heroCopy}>
              <div className={styles.eyebrow}>
                <span /> A deterministic boundary for agentic systems
              </div>
              <Heading as="h1">
                Let agents propose. <em>Let rules decide.</em>
              </Heading>
              <p className={styles.lede}>
                Put validated lifecycle policy between probabilistic intent and durable state.
                Agents choose named operations. Entity Runtime returns a replayable Decision—or a
                typed refusal that changes nothing.
              </p>
              <div className={styles.actions}>
                <Link className={styles.primaryAction} to="/docs/guide/getting-started">
                  Run the quickstart <span aria-hidden="true">↗</span>
                </Link>
                <Link className={styles.secondaryAction} to="/docs/agentic-systems">
                  Why agents need this
                </Link>
              </div>
              <div className={styles.metrics} aria-label="Core properties">
                <div>
                  <strong>Named operations</strong>
                  <span>not arbitrary state patches</span>
                </div>
                <div>
                  <strong>Typed refusals</strong>
                  <span>structured reasons, no mutation</span>
                </div>
                <div>
                  <strong>Replayable</strong>
                  <span>command, definition, result, evidence</span>
                </div>
              </div>
            </div>
            <DecisionPanel />
          </div>
        </header>

        <section className={styles.thesis}>
          <div className="container">
            <p className={styles.sectionLabel}>The boundary</p>
            <Heading as="h2">Probabilistic intent. Deterministic authority.</Heading>
            <div className={styles.thesisGrid}>
              <p>
                Models are good at interpreting requests, collecting context, and choosing what to
                try. They should not decide whether a durable record may skip a lifecycle, whether
                missing evidence counts as false, or whether an older write overwrites a newer one.
              </p>
              <p>
                Entity Runtime answers one narrow question from explicit inputs. Your trusted shell
                owns identity, authentication, time, storage, and side effects. The kernel owns the
                transition and its evidence.
              </p>
            </div>
          </div>
        </section>

        <section className={styles.flowSection}>
          <div className="container">
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.sectionLabel}>One safe path</p>
                <Heading as="h2">From proposal to durable fact.</Heading>
              </div>
              <p>
                Authority is injected at the edge. The model never chooses its own role, canonical
                state, definition, timestamp, or record identity.
              </p>
            </div>
            <ol className={styles.flow}>
              {flow.map(([number, title, text]) => (
                <li key={number}>
                  <span>{number}</span>
                  <Heading as="h3">{title}</Heading>
                  <p>{text}</p>
                </li>
              ))}
            </ol>
          </div>
        </section>

        <section className={styles.benefits}>
          <div className="container">
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.sectionLabel}>Why it helps</p>
                <Heading as="h2">A tool contract both agents and operators can trust.</Heading>
              </div>
              <Link to="/docs/guarantees">Guarantees and limits →</Link>
            </div>
            <div className={styles.benefitGrid}>
              {benefits.map((benefit) => (
                <article key={benefit.label}>
                  <span>{benefit.label}</span>
                  <Heading as="h3">{benefit.title}</Heading>
                  <p>{benefit.text}</p>
                </article>
              ))}
            </div>
          </div>
        </section>

        <section className={styles.fitSection}>
          <div className={`container ${styles.fitGrid}`}>
            <div>
              <p className={styles.sectionLabel}>Where it fits</p>
              <Heading as="h2">Use it where a proposed action must become an accountable fact.</Heading>
              <p className={styles.fitIntro}>
                The runtime is deliberately domain-neutral. A refund, deployment, planning artifact,
                claim, or access request is the same shape: fields, states, operations, rules, and
                events declared as data.
              </p>
            </div>
            <ul className={styles.fitList}>
              {fits.map((fit) => (
                <li key={fit}>{fit}</li>
              ))}
            </ul>
          </div>
        </section>

        <section className={styles.benefits}>
          <div className="container">
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.sectionLabel}>From one definition</p>
                <Heading as="h2">Give people, agents, and tools the view they need.</Heading>
              </div>
              <Link to="/docs/guide/cli">CLI reference →</Link>
            </div>
            <div className={styles.benefitGrid}>
              <article>
                <span>GRAPH</span>
                <Heading as="h3">Lifecycle and references</Heading>
                <p>Render text, Mermaid, DOT, SVG, or HTML without maintaining a second model.</p>
                <Link to="/docs/guide/graphs">See graph examples →</Link>
              </article>
              <article>
                <span>DOCS</span>
                <Heading as="h3">Human and machine contracts</Heading>
                <p>Generate entity pages, OpenAPI, and AsyncAPI from validated definitions.</p>
                <Link to="/docs/guide/generated-docs">Open the generated refund docs →</Link>
              </article>
              <article>
                <span>MCP</span>
                <Heading as="h3">Test a model against real operations</Heading>
                <p>Mount schema-derived stored tools with provenance and optimistic concurrency.</p>
                <Link to="/docs/guide/mcp">Mount entity tools →</Link>
              </article>
              <article>
                <span>RUST</span>
                <Heading as="h3">A definition-specific command</Heading>
                <p>Compile direct create, read, event, and lifecycle commands for the host.</p>
                <Link to="/docs/guide/generated-cli">Generate a Rust CLI →</Link>
              </article>
            </div>
          </div>
        </section>

        <section className={styles.notFramework}>
          <div className="container">
            <p className={styles.sectionLabel}>A precise promise</p>
            <div className={styles.notFrameworkGrid}>
              <Heading as="h2">Not another agent framework.</Heading>
              <div>
                <p>
                  Entity Runtime does not call models, plan tasks, authenticate users, choose tools,
                  publish messages, or execute side effects. It is the deterministic authority your
                  framework calls before state is allowed to change.
                </p>
                <Link className={styles.primaryAction} to="/docs/guide/agent-integration">
                  Connect an agent safely <span aria-hidden="true">↗</span>
                </Link>
              </div>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}

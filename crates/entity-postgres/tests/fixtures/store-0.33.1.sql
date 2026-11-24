--
-- PostgreSQL database dump
--

-- Dumped from database version 17.6 (Debian 17.6-2.pgdg13+1)
-- Dumped by pg_dump version 17.6 (Debian 17.6-2.pgdg13+1)

--
-- Name: __SCHEMA__; Type: SCHEMA; Schema: -; Owner: -
--

CREATE SCHEMA __SCHEMA__;

--
-- Name: entity_index_document(text); Type: FUNCTION; Schema: __SCHEMA__; Owner: -
--

CREATE FUNCTION __SCHEMA__.entity_index_document(document text) RETURNS jsonb
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $$
                     SELECT CASE WHEN strpos(document, chr(92) || 'u') = 0 THEN document::jsonb
                     ELSE replace(replace(replace(replace(document,
                         chr(92) || chr(92), chr(1)),
                         chr(92) || 'u0001', chr(92) || 'u0001' || chr(92) || 'u0001'),
                         chr(92) || 'u0000', chr(92) || 'u0001' || chr(92) || 'u0002'),
                         chr(1), chr(92) || chr(92))::jsonb END
                 $$;

--
-- Name: events; Type: TABLE; Schema: __SCHEMA__; Owner: -
--

CREATE TABLE __SCHEMA__.events (
    entity text NOT NULL,
    id text NOT NULL,
    revision bigint NOT NULL,
    "position" bigint NOT NULL,
    document text NOT NULL
);

--
-- Name: history; Type: TABLE; Schema: __SCHEMA__; Owner: -
--

CREATE TABLE __SCHEMA__.history (
    entity text NOT NULL,
    id text NOT NULL,
    "position" bigint NOT NULL,
    kind text NOT NULL,
    record_id text NOT NULL,
    document text NOT NULL,
    CONSTRAINT history_kind_check CHECK ((kind = ANY (ARRAY['decision'::text, 'observation'::text])))
);

--
-- Name: instances; Type: TABLE; Schema: __SCHEMA__; Owner: -
--

CREATE TABLE __SCHEMA__.instances (
    entity text NOT NULL,
    id text NOT NULL,
    revision bigint NOT NULL,
    document text NOT NULL
);

--
-- Name: legacy_origins; Type: TABLE; Schema: __SCHEMA__; Owner: -
--

CREATE TABLE __SCHEMA__.legacy_origins (
    entity text NOT NULL,
    id text NOT NULL,
    revision bigint NOT NULL
);

--
-- Name: provider_sequences; Type: TABLE; Schema: __SCHEMA__; Owner: -
--

CREATE TABLE __SCHEMA__.provider_sequences (
    namespace text NOT NULL,
    next_value bigint NOT NULL,
    CONSTRAINT provider_sequences_next_value_check CHECK ((next_value >= 0))
);

--
-- Data for Name: events; Type: TABLE DATA; Schema: __SCHEMA__; Owner: -
--

INSERT INTO __SCHEMA__.events VALUES ('ticket', 't3', 1, 0, '{"entity":"ticket","version":1,"id":"t3","revision":1,"type":"TicketOpened","from_state":null,"to_state":"open","changed":{"title":"Bare"},"args":{"title":"Bare"},"payload":{"ticket":"t3"}}');

--
-- Data for Name: history; Type: TABLE DATA; Schema: __SCHEMA__; Owner: -
--

INSERT INTO __SCHEMA__.history VALUES ('ticket', 't1', 0, 'decision', 'r-t1-1', '{"record_id":"r-t1-1","recorded_at":"2026-11-23T10:00:00Z","correlation":"fixture-0.33.1","causation":null,"actor":"fixture","record":{"definition":{"entity":"ticket","version":1,"schema":{"fields":{"title":{"type":"string","required":true,"min_length":null,"max_length":null,"min":null,"max":null,"values":[],"items":null,"properties":{},"additional_properties":false,"entity":null,"inverse":null,"acyclic":null}},"additional_fields":false},"lifecycle":{"initial":"open","states":["open","closed"]},"invariants":[],"create":{"emit":{"type":"TicketOpened","payload":{"ticket":"$id"}}},"operations":{"close":{"arguments":{"fields":{},"additional_fields":false},"transitions":[{"from":"open","to":"closed"}],"preconditions":[],"set":{},"emits":[{"type":"TicketClosed","payload":{"ticket":"$id"}}]}},"projections":{}},"command":{"command":"create","fields":{"title":"First"}},"entity":"ticket","id":"t1","revision":1,"from_state":null,"to_state":"open","result":{"entity":"ticket","version":1,"id":"t1","lifecycle_state":"open","revision":1,"fields":{"title":"First"}},"changed":{"title":"First"},"events":[{"entity":"ticket","version":1,"id":"t1","revision":1,"type":"TicketOpened","from_state":null,"to_state":"open","changed":{"title":"First"},"args":{"title":"First"},"payload":{"ticket":"t1"}}]}}');
INSERT INTO __SCHEMA__.history VALUES ('ticket', 't1', 1, 'decision', 'r-t1-2', '{"record_id":"r-t1-2","recorded_at":"2026-11-23T10:01:00Z","correlation":"fixture-0.33.1","causation":null,"actor":"fixture","record":{"definition":{"entity":"ticket","version":1,"schema":{"fields":{"title":{"type":"string","required":true,"min_length":null,"max_length":null,"min":null,"max":null,"values":[],"items":null,"properties":{},"additional_properties":false,"entity":null,"inverse":null,"acyclic":null}},"additional_fields":false},"lifecycle":{"initial":"open","states":["open","closed"]},"invariants":[],"create":{"emit":{"type":"TicketOpened","payload":{"ticket":"$id"}}},"operations":{"close":{"arguments":{"fields":{},"additional_fields":false},"transitions":[{"from":"open","to":"closed"}],"preconditions":[],"set":{},"emits":[{"type":"TicketClosed","payload":{"ticket":"$id"}}]}},"projections":{}},"command":{"command":"execute","operation":"close","arguments":{}},"entity":"ticket","id":"t1","revision":2,"from_state":"open","to_state":"closed","result":{"entity":"ticket","version":1,"id":"t1","lifecycle_state":"closed","revision":2,"fields":{"title":"First"}},"changed":{},"events":[{"entity":"ticket","version":1,"id":"t1","revision":2,"type":"TicketClosed","from_state":"open","to_state":"closed","changed":{},"args":{},"payload":{"ticket":"t1"}}]}}');
INSERT INTO __SCHEMA__.history VALUES ('ticket', 't1', 2, 'observation', 'o-t1-1', '{"entity":"ticket","id":"t1","revision":2,"envelope":{"record_id":"o-t1-1","recorded_at":"2026-11-23T10:02:00Z","correlation":"fixture-0.33.1","causation":null,"actor":"fixture","record":{"checked":true}}}');
INSERT INTO __SCHEMA__.history VALUES ('ticket', 't2', 0, 'decision', 'r-t2-1', '{"record_id":"r-t2-1","recorded_at":"2026-11-23T10:03:00Z","correlation":"fixture-0.33.1","causation":null,"actor":"fixture","record":{"definition":{"entity":"ticket","version":1,"schema":{"fields":{"title":{"type":"string","required":true,"min_length":null,"max_length":null,"min":null,"max":null,"values":[],"items":null,"properties":{},"additional_properties":false,"entity":null,"inverse":null,"acyclic":null}},"additional_fields":false},"lifecycle":{"initial":"open","states":["open","closed"]},"invariants":[],"create":{"emit":{"type":"TicketOpened","payload":{"ticket":"$id"}}},"operations":{"close":{"arguments":{"fields":{},"additional_fields":false},"transitions":[{"from":"open","to":"closed"}],"preconditions":[],"set":{},"emits":[{"type":"TicketClosed","payload":{"ticket":"$id"}}]}},"projections":{}},"command":{"command":"create","fields":{"title":"Second"}},"entity":"ticket","id":"t2","revision":1,"from_state":null,"to_state":"open","result":{"entity":"ticket","version":1,"id":"t2","lifecycle_state":"open","revision":1,"fields":{"title":"Second"}},"changed":{"title":"Second"},"events":[{"entity":"ticket","version":1,"id":"t2","revision":1,"type":"TicketOpened","from_state":null,"to_state":"open","changed":{"title":"Second"},"args":{"title":"Second"},"payload":{"ticket":"t2"}}]}}');

--
-- Data for Name: instances; Type: TABLE DATA; Schema: __SCHEMA__; Owner: -
--

INSERT INTO __SCHEMA__.instances VALUES ('ticket', 't1', 2, '{"entity":"ticket","version":1,"id":"t1","lifecycle_state":"closed","revision":2,"fields":{"title":"First"}}');
INSERT INTO __SCHEMA__.instances VALUES ('ticket', 't2', 1, '{"entity":"ticket","version":1,"id":"t2","lifecycle_state":"open","revision":1,"fields":{"title":"Second"}}');
INSERT INTO __SCHEMA__.instances VALUES ('ticket', 't3', 1, '{"entity":"ticket","version":1,"id":"t3","lifecycle_state":"open","revision":1,"fields":{"title":"Bare"}}');

--
-- Data for Name: legacy_origins; Type: TABLE DATA; Schema: __SCHEMA__; Owner: -
--

INSERT INTO __SCHEMA__.legacy_origins VALUES ('ticket', 't3', 1);

--
-- Data for Name: provider_sequences; Type: TABLE DATA; Schema: __SCHEMA__; Owner: -
--

--
-- Name: events events_pkey; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (entity, id, revision, "position");

--
-- Name: history history_pkey; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.history
    ADD CONSTRAINT history_pkey PRIMARY KEY (entity, id, "position");

--
-- Name: history history_record_id_key; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.history
    ADD CONSTRAINT history_record_id_key UNIQUE (record_id);

--
-- Name: instances instances_pkey; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.instances
    ADD CONSTRAINT instances_pkey PRIMARY KEY (entity, id);

--
-- Name: legacy_origins legacy_origins_pkey; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.legacy_origins
    ADD CONSTRAINT legacy_origins_pkey PRIMARY KEY (entity, id);

--
-- Name: provider_sequences provider_sequences_pkey; Type: CONSTRAINT; Schema: __SCHEMA__; Owner: -
--

ALTER TABLE ONLY __SCHEMA__.provider_sequences
    ADD CONSTRAINT provider_sequences_pkey PRIMARY KEY (namespace);

--
-- Name: instances_document_index; Type: INDEX; Schema: __SCHEMA__; Owner: -
--

CREATE INDEX instances_document_index ON __SCHEMA__.instances USING gin (__SCHEMA__.entity_index_document(document) jsonb_path_ops);

--
-- Name: instances_id_byte_order; Type: INDEX; Schema: __SCHEMA__; Owner: -
--

CREATE INDEX instances_id_byte_order ON __SCHEMA__.instances USING btree (entity, id COLLATE "C");

--
-- PostgreSQL database dump complete
--


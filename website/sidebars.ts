import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'category',
      label: 'Start here',
      collapsed: false,
      items: [
        {type: 'doc', id: 'intro', label: 'What Entity Runtime does'},
        {type: 'doc', id: 'agentic-systems', label: 'Why agents need it'},
        {type: 'doc', id: 'guide/getting-started', label: 'Refund quickstart'},
      ],
    },
    {
      type: 'category',
      label: 'Build',
      collapsed: false,
      items: [
        {type: 'doc', id: 'guide/modeling', label: 'Model policy as data'},
        {type: 'doc', id: 'guide/graphs', label: 'Render graphs'},
        {type: 'doc', id: 'guide/agent-integration', label: 'Connect an agent safely'},
        {type: 'doc', id: 'guide/mcp', label: 'Mount MCP tools'},
        {type: 'doc', id: 'guide/generated-docs', label: 'Generate entity docs'},
        {type: 'doc', id: 'guide/generated-cli', label: 'Generate a Rust CLI'},
        {type: 'doc', id: 'guide/storage', label: 'Persist and replay decisions'},
      ],
    },
    {
      type: 'category',
      label: 'Reference',
      collapsed: false,
      items: [
        {type: 'doc', id: 'guide/definitions', label: 'Definition language'},
        {type: 'doc', id: 'guide/cli', label: 'CLI'},
        {type: 'doc', id: 'guide/refusals', label: 'Typed refusals'},
        {type: 'doc', id: 'guide/library', label: 'Rust libraries'},
        {type: 'doc', id: 'guarantees', label: 'Guarantees and limits'},
      ],
    },
    {
      type: 'category',
      label: 'Operate',
      collapsed: false,
      items: [
        {type: 'doc', id: 'guide/file-store-migration', label: 'File Store v2 migration'},
      ],
    },
  ],
};

export default sidebars;

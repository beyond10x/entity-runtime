import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {type: 'doc', id: 'guide/getting-started', label: 'Getting started'},
    {
      type: 'category',
      label: 'Guide',
      collapsed: false,
      items: [
        {type: 'doc', id: 'guide/definitions', label: 'The definition language'},
        {type: 'doc', id: 'guide/cli', label: 'The entity command'},
        {type: 'doc', id: 'guide/library', label: 'The library'},
      ],
    },
    {type: 'doc', id: 'VISION', label: 'Vision'},
    {type: 'doc', id: 'requirements', label: 'Requirements register'},
    {type: 'doc', id: 'roadmap', label: 'Roadmap (proposed)'},
    {
      type: 'doc',
      id: 'reviews/2026-08-25-adversarial-review',
      label: 'Adversarial review (0.1.0)',
    },
    {
      type: 'category',
      label: 'Design',
      collapsed: false,
      items: [
        {type: 'doc', id: 'design/kernel-v0.1', label: 'The kernel (normative)'},
        {
          type: 'doc',
          id: 'design/engineering-protocols-adoption-v0.1',
          label: 'Driving engineering-protocols (proposed)',
        },
      ],
    },
  ],
};

export default sidebars;

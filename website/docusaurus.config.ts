import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import docsSystemPlugin from '@beyond10x/docs-system/docusaurus';

// Public product documentation lives here under `website/docs`. The repository-root `docs/` tree
// is the engineering record — requirements, designs, plans and reviews — and is deliberately not
// a website content source. Markdown is parsed as CommonMark because these pages need no MDX.
//
// GitHub Pages must publish the *built output* through the Actions workflow, never the `/docs`
// folder of the branch: that would serve the raw tree at the public URL.

const config: Config = {
  title: 'Entity Runtime',
  tagline: 'Let agents propose. Let deterministic rules decide.',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  url: 'https://beyond10x.github.io',
  baseUrl: '/entity-runtime/',
  organizationName: 'beyond10x',
  projectName: 'entity-runtime',
  trailingSlash: false,

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  markdown: {
    format: 'detect',
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          path: 'docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/beyond10x/entity-runtime/edit/main/website/docs/',
          showLastUpdateTime: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themes: ['@docusaurus/theme-mermaid'],
  plugins: [docsSystemPlugin],

  themeConfig: {
    image: 'img/social-card.svg',
    metadata: [
      {
        name: 'keywords',
        content:
          'agent safety, AI agents, deterministic tools, entity runtime, lifecycle, domain events, schema-driven, Rust, YAML, replay',
      },
    ],
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Entity Runtime',
      hideOnScroll: true,
      logo: {
        alt: 'Entity Runtime mark',
        src: 'img/mark.svg',
      },
      items: [
        {href: 'https://beyond10x.github.io/getting-started/', label: 'beyond10x', position: 'left'},
        {href: 'https://beyond10x.github.io/getting-started/ecosystem', label: 'Ecosystem', position: 'left'},
        {to: '/docs/agentic-systems', label: 'Why for agents', position: 'left'},
        {to: '/docs/guide/getting-started', label: 'Quickstart', position: 'left'},
        {
          label: 'Build',
          position: 'left',
          items: [
            {to: '/docs/guide/modeling', label: 'Model policy as data'},
            {to: '/docs/guide/graphs', label: 'Render graphs'},
            {to: '/docs/guide/agent-integration', label: 'Connect an agent'},
            {to: '/docs/guide/mcp', label: 'Mount MCP tools'},
            {to: '/docs/guide/generated-docs', label: 'Generate entity docs'},
            {to: '/docs/guide/generated-cli', label: 'Generate a Rust CLI'},
          ],
        },
        {
          label: 'Reference',
          position: 'left',
          items: [
            {to: '/docs/guide/definitions', label: 'Definition language'},
            {to: '/docs/guide/cli', label: 'CLI'},
            {to: '/docs/guide/refusals', label: 'Typed refusals'},
            {to: '/docs/guide/library', label: 'Rust libraries'},
            {to: '/docs/guarantees', label: 'Guarantees and limits'},
          ],
        },
        {
          href: 'https://github.com/beyond10x/entity-runtime',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Use it',
          items: [
            {label: 'Refund quickstart', to: '/docs/guide/getting-started'},
            {label: 'Model policy as data', to: '/docs/guide/modeling'},
            {label: 'Connect an agent', to: '/docs/guide/agent-integration'},
            {label: 'Persist decisions', to: '/docs/guide/storage'},
          ],
        },
        {
          title: 'Reference',
          items: [
            {label: 'Definition language', to: '/docs/guide/definitions'},
            {label: 'CLI', to: '/docs/guide/cli'},
            {label: 'Typed refusals', to: '/docs/guide/refusals'},
            {label: 'Guarantees and limits', to: '/docs/guarantees'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'GitHub repository', href: 'https://github.com/beyond10x/entity-runtime'},
            {label: 'Releases', href: 'https://github.com/beyond10x/entity-runtime/releases'},
            {label: 'Apache-2.0 license', href: 'https://github.com/beyond10x/entity-runtime/blob/main/LICENSE'},
          ],
        },
      ],
      copyright: `© ${new Date().getFullYear()} beyond10x · Agent intent in. Deterministic decision out.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'yaml', 'json', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;

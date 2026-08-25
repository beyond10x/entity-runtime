import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// The site renders the repository's own `docs/` tree — the guide, the vision, the requirements
// register and the designs — so there is exactly one copy of every document. Markdown files are
// parsed as CommonMark (`format: 'detect'`): the register and the designs are full of `{ }` and
// `<path>` that MDX would read as code, and nothing here needs MDX.
//
// GitHub Pages must publish the *built output* through the Actions workflow, never the `/docs`
// folder of the branch: that would serve the raw tree at the public URL.

const config: Config = {
  title: 'Entity Runtime',
  tagline: 'Entity types declared as data, decided by an IO-free kernel',
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
          path: '../docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/beyond10x/entity-runtime/edit/main/docs/',
          showLastUpdateTime: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/social-card.svg',
    metadata: [
      {
        name: 'keywords',
        content:
          'entity runtime, state machine, lifecycle, domain events, schema-driven, deterministic kernel, Rust, YAML, event sourcing',
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
        {to: '/docs/guide/getting-started', label: 'Get started', position: 'left'},
        {to: '/docs/guide/definitions', label: 'Definitions', position: 'left'},
        {to: '/docs/guide/cli', label: 'CLI', position: 'left'},
        {to: '/docs/requirements', label: 'Requirements', position: 'left'},
        {to: '/docs/design/kernel-v0.1', label: 'Design', position: 'left'},
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
            {label: 'Getting started', to: '/docs/guide/getting-started'},
            {label: 'The definition language', to: '/docs/guide/definitions'},
            {label: 'The entity command', to: '/docs/guide/cli'},
            {label: 'The library', to: '/docs/guide/library'},
          ],
        },
        {
          title: 'Read it',
          items: [
            {label: 'Vision', to: '/docs/VISION'},
            {label: 'Requirements register', to: '/docs/requirements'},
            {label: 'Kernel design', to: '/docs/design/kernel-v0.1'},
            {label: 'Driving engineering-protocols', to: '/docs/design/engineering-protocols-adoption-v0.1'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'GitHub repository', href: 'https://github.com/beyond10x/entity-runtime'},
            {label: 'engineering-protocols', href: 'https://beyond10x.github.io/engineering-protocols/'},
            {label: 'beyond10x Atlas', href: 'https://github.com/beyond10x/atlas'},
          ],
        },
      ],
      copyright: `© ${new Date().getFullYear()} beyond10x · A refusal changes nothing.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'yaml', 'json', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;

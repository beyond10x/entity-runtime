import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {withProductSite} from '@beyond10x/docs-system/product-site';

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
  deploymentBranch: 'gh-pages',
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
          editUrl: 'https://github.com/beyond10x/entity-runtime/tree/main/website/',
        },
        blog: false,
      } satisfies Preset.Options,
    ],
  ],

  themes: ['@docusaurus/theme-mermaid'],

  themeConfig: {
    image: 'img/social-card.svg',
    navbar: {
      title: 'Entity Runtime',
      logo: {
        alt: 'Entity Runtime mark',
        src: 'img/mark.svg',
      },
      items: [
        {to: '/docs/', label: 'Documentation', position: 'left'},
        {to: '/docs/getting-started', label: 'Get started', position: 'left'},
        {to: '/docs/reference/cli', label: 'CLI', position: 'left'},
        {to: '/docs/status', label: 'Status', position: 'left'},
        {href: 'https://github.com/beyond10x/entity-runtime', label: 'GitHub', position: 'right'},
      ],
    },
    footer: {
      links: [
        {
          title: 'Documentation',
          items: [
            {label: 'Overview', to: '/docs/'},
            {label: 'Getting started', to: '/docs/getting-started'},
            {label: 'The decision boundary', to: '/docs/concepts/decision-boundary'},
            {label: 'Storage and replay', to: '/docs/concepts/storage'},
            {label: 'Connect an agent', to: '/docs/guides/connect-an-agent'},
            {label: 'Definition language', to: '/docs/reference/definitions'},
            {label: 'entity CLI', to: '/docs/reference/cli'},
            {label: 'Status', to: '/docs/status'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'Source', href: 'https://github.com/beyond10x/entity-runtime'},
            {label: 'Releases', href: 'https://github.com/beyond10x/entity-runtime/releases'},
            {label: 'Changelog', href: 'https://github.com/beyond10x/entity-runtime/blob/main/CHANGELOG.md'},
          ],
        },
      ],
      copyright: 'A beyond10x project. Entity Runtime · Apache-2.0.',
    },
  } satisfies Preset.ThemeConfig,
};

export default withProductSite(config, {landing: './product.json', mark: 'ER'});

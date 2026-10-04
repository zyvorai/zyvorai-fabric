import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const config: Config = {
  title: 'Zyvor Fabric',
  tagline: 'Private cloud control plane for Linux — VMs, networking, storage, and security from one daemon.',
  favicon: 'img/favicon.svg',

  future: {
    v4: true, // Improve compatibility with the upcoming Docusaurus v4
  },

  url: 'https://zyvorai.github.io',
  baseUrl: '/zyvorai-fabric/',

  organizationName: 'zyvorai',
  projectName: 'zyvorai-fabric',

  onBrokenLinks: 'warn',

  markdown: {
    // The existing docs/ corpus was written as plain Markdown for GitHub
    // rendering, not MDX — force CommonMark parsing so raw `<`/`!`/`{`
    // characters in prose don't get misread as JSX.
    format: 'md',
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  // Serve the repo's existing dashboard screenshot in place instead of
  // duplicating it into website/static — see website/README.md.
  staticDirectories: ['static', '../docs/assets'],

  headTags: [
    {
      tagName: 'link',
      attributes: {
        rel: 'apple-touch-icon',
        sizes: '180x180',
        href: '/fabric/img/apple-touch-icon.png',
      },
    },
  ],

  plugins: [
    [
      '@easyops-cn/docusaurus-search-local',
      {
        // Self-hosted, build-time index — no external Algolia account/API
        // keys needed. See website/README.md.
        hashed: true,
        indexDocs: true,
        indexPages: false,
        docsRouteBasePath: '/docs',
        language: 'en',
      },
    ],
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          // Serve the repo's existing docs/ tree directly rather than
          // hand-curating a separate copy — see website/README.md.
          path: '../docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/zyvorai/zyvorai-fabric/tree/main/docs/',
          // docs/keep/README.md is the GitHub landing page for Keep. The site
          // already has /keep and docs/keep/KEEP.md (the folder index), so
          // keep the README out of the docs build to avoid a duplicate route.
          exclude: [
            '**/_*.{js,jsx,ts,tsx,md,mdx}',
            '**/_*/**',
            '**/*.test.{js,jsx,ts,tsx}',
            '**/__tests__/**',
            'keep/README.md',
          ],
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/social-card.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: false,
    },
    navbar: {
      title: 'Zyvor Fabric',
      logo: {
        alt: 'Zyvor Fabric',
        src: 'img/favicon.svg',
      },
      hideOnScroll: false,
      items: [
        {
          to: '/keep',
          label: 'Keep',
          position: 'right',
        },
        {
          to: '/keep/phones',
          label: 'Phones',
          position: 'right',
        },
        {
          to: '/#matrix',
          label: 'Matrix',
          position: 'right',
        },
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'right',
          label: 'Docs',
        },
        {
          href: 'https://github.com/zyvorai/zyvorai-fabric',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {label: 'Quick start', to: '/docs/getting-started/Quick-Start'},
            {label: 'Keep', to: '/keep'},
            {label: 'Keep for phone makers', to: '/keep/phones'},
            {label: 'Keep use cases', to: '/keep/packs'},
            {label: 'Fabric vs the field', to: '/#matrix'},
            {label: 'Keep docs', to: '/docs/keep/'},
            {label: 'Tutorial 17 — PDF brief', to: '/docs/tutorials/keep-pdf-brief'},
            {label: 'Product overview', to: '/docs/PRODUCT_OVERVIEW'},
            {label: 'FAQ', to: '/docs/quick-reference/faq'},
          ],
        },
        {
          title: 'Project',
          items: [
            {label: 'GitHub', href: 'https://github.com/zyvorai/zyvorai-fabric'},
            {
              label: 'Changelog',
              href: 'https://github.com/zyvorai/zyvorai-fabric/blob/main/CHANGELOG.md',
            },
            {
              label: 'License (Apache-2.0)',
              href: 'https://github.com/zyvorai/zyvorai-fabric/blob/main/LICENSE',
            },
          ],
        },
        {
          title: 'Zyvor Enterprise',
          items: [
            {label: 'zyvor.dev', href: 'https://zyvor.dev'},
            {label: 'Keep on zyvor.dev', href: 'https://zyvor.dev/keep'},
            {label: 'sales@zyvor.dev', href: 'mailto:sales@zyvor.dev'},
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Zyvor. Zyvor Fabric core is Apache-2.0 licensed.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;

// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightLinksValidator from 'starlight-links-validator';

const GITHUB = 'https://github.com/mobiler/mobiler';

// https://astro.build/config
export default defineConfig({
	site: 'https://mobiler.rs',
	integrations: [
		starlight({
			title: 'Mobiler',
			description: 'Build Android, iOS and web apps from one Rust core, rendered to real native widgets.',
			logo: { src: './src/assets/logo.svg', replacesTitle: false },
			favicon: '/favicon.svg',
			customCss: ['./src/styles/custom.css'],
			social: [{ icon: 'github', label: 'GitHub', href: GITHUB }],
			editLink: { baseUrl: `${GITHUB}/edit/main/site/` },
			lastUpdated: false,
			plugins: [starlightLinksValidator({ errorOnLocalLinks: false })],
			sidebar: [
				{
					label: 'Start here',
					items: [
						{ label: "What's shared", slug: 'why/shared' },
						{ label: 'How it works', slug: 'why/how-it-works' },
						{ label: 'Get started', slug: 'start/get-started' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'One core for mobile, web and server', slug: 'guides/full-stack' },
						{ label: 'Add a web client', slug: 'guides/web' },
						{ label: 'Capabilities and plugins', slug: 'guides/capabilities' },
						{ label: 'Theming', slug: 'guides/theming' },
						{ label: 'mobiler.toml options', slug: 'guides/configuration' },
						{ label: 'Upgrading an app', slug: 'guides/upgrading' },
					],
				},
				{
					label: 'Examples',
					items: [{ label: 'Demo apps', slug: 'examples' }],
				},
				{
					label: 'Tutorial: build Saldo',
					items: [
						{ label: 'Try Saldo', slug: 'try-saldo' },
						{ autogenerate: { directory: 'tutorial' } },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Widgets', slug: 'reference/widgets' },
						{ label: 'Built-in capabilities', slug: 'reference/capabilities' },
						{ label: 'Plugins', slug: 'reference/plugins' },
						{ label: 'CLI', slug: 'reference/cli' },
						{ label: 'Design decisions', slug: 'reference/decisions' },
					],
				},
			],
		}),
	],
});

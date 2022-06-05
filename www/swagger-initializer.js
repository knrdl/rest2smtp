window.onload = function () {
    window.ui = SwaggerUIBundle({
        url: "./openapi.yaml",
        dom_id: '#swagger-ui',
        deepLinking: true,
        docExpansion: 'full',
        displayRequestDuration: true,
        presets: [
            SwaggerUIBundle.presets.apis,
        ],
        plugins: [],
        layout: "BaseLayout"
    });
};

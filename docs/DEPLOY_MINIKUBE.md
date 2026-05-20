# Minikube Deployment

> **Goal**: `kubectl apply -k deploy/minikube` deploys a production-ready yas-mcp server in under 60 seconds.

## Quickstart

```bash
# 1. Start minikube (if not running)
minikube start --cpus=4 --memory=8192

# 2. Deploy yas-mcp with example API
kubectl apply -k deploy/minikube/examples/todo-app

# 3. Check status
kubectl -n yas-mcp get all

# 4. Port-forward to access
kubectl -n yas-mcp port-forward svc/yas-mcp 3000:3000

# 5. Test
curl -X POST http://localhost:3000/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
```

## Directory Structure

```
deploy/
├── minikube/
│   ├── base/                          # Base Kustomize layer
│   │   ├── kustomization.yaml
│   │   ├── namespace.yaml
│   │   ├── service-account.yaml
│   │   ├── deployment.yaml
│   │   ├── service.yaml
│   │   └── ingress.yaml
│   ├── examples/                      # Example overlays
│   │   ├── todo-app/
│   │   │   ├── kustomization.yaml
│   │   │   ├── configmap.yaml         # OpenAPI spec + adjustments
│   │   │   └── secret.yaml            # OIDC secrets (sealed)
│   │   └── petstore/
│   │       ├── kustomization.yaml
│   │       └── configmap.yaml
│   └── overlays/                      # Environment overlays
│       ├── dev/
│       │   └── kustomization.yaml     # Debug logging, no TLS
│       ├── staging/
│       │   └── kustomization.yaml     # TLS, auth, resource limits
│       └── prod/
│           └── kustomization.yaml     # HA, HPA, PDB, sealed secrets
├── helm/
│   └── yas-mcp/                       # Optional Helm chart
│       ├── Chart.yaml
│       ├── values.yaml
│       └── templates/
└── docker-compose/
    └── docker-compose.yml             # Existing, keep maintained
```

## Base Manifests

### `deployment.yaml`

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: yas-mcp
  namespace: yas-mcp
spec:
  replicas: 2
  selector:
    matchLabels:
      app: yas-mcp
  template:
    metadata:
      labels:
        app: yas-mcp
      annotations:
        prometheus.io/scrape: "true"
        prometheus.io/port: "3000"
        prometheus.io/path: "/metrics"
    spec:
      serviceAccountName: yas-mcp
      containers:
        - name: yas-mcp
          image: yas-mcp:latest
          imagePullPolicy: IfNotPresent
          ports:
            - containerPort: 3000
              name: http
          envFrom:
            - configMapRef:
                name: yas-mcp-config
            - secretRef:
                name: yas-mcp-secrets
                optional: true
          resources:
            limits:
              memory: "512Mi"
              cpu: "1"
            requests:
              memory: "64Mi"
              cpu: "100m"
          readinessProbe:
            httpGet:
              path: /health
              port: 3000
            initialDelaySeconds: 3
            periodSeconds: 5
          livenessProbe:
            httpGet:
              path: /health
              port: 3000
            initialDelaySeconds: 10
            periodSeconds: 15
          volumeMounts:
            - name: openapi-spec
              mountPath: /app/config
              readOnly: true
      volumes:
        - name: openapi-spec
          configMap:
            name: yas-mcp-openapi
```

### `service.yaml`

```yaml
apiVersion: v1
kind: Service
metadata:
  name: yas-mcp
  namespace: yas-mcp
spec:
  selector:
    app: yas-mcp
  ports:
    - name: http
      port: 3000
      targetPort: 3000
```

### `ingress.yaml`

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: yas-mcp
  namespace: yas-mcp
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
spec:
  ingressClassName: nginx
  tls:
    - hosts:
        - mcp.example.com
      secretName: yas-mcp-tls
  rules:
    - host: mcp.example.com
      http:
        paths:
          - path: /
            pathType: Prefix
            backend:
              service:
                name: yas-mcp
                port:
                  number: 3000
```

### Example Overlay: Todo App

```yaml
# deploy/minikube/examples/todo-app/configmap.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: yas-mcp-openapi
  namespace: yas-mcp
data:
  openapi.yaml: |
    openapi: 3.0.0
    info:
      title: Todo API
      version: 1.0.0
    paths:
      /todos:
        get:
          operationId: listTodos
          summary: List all todos
          ...
        post:
          operationId: createTodo
          summary: Create a todo
          ...
  adjustments.yaml: |
    routes:
      - path: /todos
        methods: [GET, POST]
```

## Multi-Instance Deployment

For surfacing multiple APIs, deploy multiple yas-mcp instances:

```bash
# Deploy instance for API-A
kubectl apply -k deploy/minikube/examples/api-a

# Deploy instance for API-B
kubectl apply -k deploy/minikube/examples/api-b
```

Each instance gets its own:
- Deployment with unique name
- ConfigMap with its own OpenAPI spec
- Service (can share ingress with path-based routing)

```yaml
# Shared ingress routing multiple APIs
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: yas-mcp-multi
  namespace: yas-mcp
spec:
  rules:
    - host: mcp.example.com
      http:
        paths:
          - path: /api-a
            pathType: Prefix
            backend:
              service:
                name: yas-mcp-api-a
                port:
                  number: 3000
          - path: /api-b
            pathType: Prefix
            backend:
              service:
                name: yas-mcp-api-b
                port:
                  number: 3000
```

## Minikube-Specific Optimizations

### Image Build & Load

```bash
# Build image directly in minikube's Docker daemon
eval $(minikube docker-env)
docker build -t yas-mcp:latest .

# Or build outside and load
docker build -t yas-mcp:latest .
minikube image load yas-mcp:latest
```

### Makefile Target

```makefile
# Makefile additions
.PHONY: minikube-deploy minikube-deploy-example minikube-clean

MINIKUBE_NS = yas-mcp

minikube-deploy:
	kubectl apply -k deploy/minikube/base

minikube-deploy-example:
	kubectl apply -k deploy/minikube/examples/todo-app

minikube-clean:
	kubectl delete namespace $(MINIKUBE_NS)

minikube-status:
	kubectl -n $(MINIKUBE_NS) get all,ingress,configmap,secret
```

## Health + Metrics

```
┌─────────────┐     GET /health      ┌──────────┐
│  K8s Probe  │ ────────────────────►│ yas-mcp  │
└─────────────┘◄─────────────────────│  :3000   │
               │  200 + JSON body    └──────────┘
               │
┌─────────────┐     GET /metrics     ┌──────────┐
│ Prometheus  │ ────────────────────►│ yas-mcp  │
└─────────────┘◄─────────────────────│  :3000   │
               │  OpenMetrics text   └──────────┘
```

`GET /health` response:
```json
{
  "status": "healthy",
  "version": "0.2.0",
  "tools": 5,
  "uptime_sec": 84321,
  "oidc_providers": {
    "corporate-sso": "healthy",
    "partner-api": "degraded"
  },
  "upstream": {
    "http://api.example.com": "healthy"
  }
}
```

## Environment Variables (ConfigMap)

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: yas-mcp-config
  namespace: yas-mcp
data:
  YAS_MCP_SERVER_MODE: "http"
  YAS_MCP_SERVER_HOST: "0.0.0.0"
  YAS_MCP_SERVER_PORT: "3000"
  YAS_MCP_LOGGING_LEVEL: "info"
  YAS_MCP_LOGGING_FORMAT: "json"
  YAS_MCP_ENDPOINT_BASE_URL: "http://todo-api:8080"
  YAS_MCP_SWAGGER_FILE: "/app/config/openapi.yaml"
  YAS_MCP_ADJUSTMENTS_FILE: "/app/config/adjustments.yaml"
```

## Secrets

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: yas-mcp-secrets
  namespace: yas-mcp
type: Opaque
stringData:
  OIDC_CLIENT_ID: "my-client-id"
  OIDC_CLIENT_SECRET: "my-client-secret"
```

For production, use [sealed-secrets](https://github.com/bitnami-labs/sealed-secrets) or [External Secrets Operator](https://external-secrets.io/).

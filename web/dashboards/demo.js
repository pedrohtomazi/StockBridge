/*
 * StockBridge - DADOS DE DEMONSTRAÇÃO (fictícios).
 * Só são usados quando o usuário clica em "Usar dados de demonstração".
 * Têm o mesmo formato das respostas da API; a página marca tudo como DEMO e os
 * arquivos exportados saem com o prefixo DEMO_.
 */
(function (raiz) {
    const P = {
        a: { productName: 'Placa Principal TV LG 50UK6500', sku: 'LG-PL-50UK6500-MAIN', cost: 120, price: 280 },
        b: { productName: 'Placa Fonte TV Samsung 43T5300', sku: 'SAM-PS-43T5300-POWER', cost: 85, price: 195 },
        c: { productName: 'Placa T-Con LG 55NANO81BNA', sku: 'LG-TCON-55NANO81', cost: 60, price: 150 }
    };
    const mov = (n, p, type, quantity, createdAt) => ({ id: `demo-${n}`, ...P[p], type, quantity, createdAt });
    const movimentos = [
        mov(1, 'a', 'IN', 50, '2026-08-25T10:00:00Z'), mov(2, 'b', 'IN', 50, '2026-08-26T10:00:00Z'),
        mov(3, 'c', 'IN', 30, '2026-08-27T10:00:00Z'), mov(4, 'a', 'OUT', 12, '2026-09-02T14:00:00Z'),
        mov(5, 'b', 'OUT', 20, '2026-09-03T14:00:00Z'), mov(6, 'c', 'OUT', 10, '2026-09-05T14:00:00Z'),
        mov(7, 'a', 'OUT', 20, '2026-09-12T14:00:00Z'), mov(8, 'b', 'OUT', 22, '2026-09-15T14:00:00Z'),
        mov(9, 'c', 'OUT', 17, '2026-09-18T14:00:00Z')
    ].reverse(); // a API devolve do mais recente para o mais antigo
    const locais = [
        { id: 'DEMO-A03', capacidade: 40, itens: [{ product_id: 'demo-pa', product_name: P.a.productName, sku: P.a.sku, quantity: 18 }] },
        { id: 'DEMO-B01', capacidade: 25, itens: [{ product_id: 'demo-pb', product_name: P.b.productName, sku: P.b.sku, quantity: 8 }] },
        { id: 'DEMO-A01', capacidade: 10, itens: [{ product_id: 'demo-pc', product_name: P.c.productName, sku: P.c.sku, quantity: 3 }] }
    ];
    raiz.StockBridgeDemo = { movimentos, locais };
})(window);

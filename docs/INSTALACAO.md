# StockBridge: instalação na empresa

Você recebe **um arquivo só**: `StockBridge.exe`. Não precisa instalar mais nada.

## 1. Escolha o computador servidor
Um computador da empresa guarda os dados e atende os outros. Ele precisa:
- ficar **ligado e com o StockBridge aberto** enquanto alguém usa o sistema;
- estar na mesma rede (mesmo roteador/Wi-Fi) que os outros computadores.

## 2. No computador servidor
1. Crie uma pasta, por exemplo `C:\StockBridge`, e coloque o `StockBridge.exe` dentro.
2. Dê dois cliques no `StockBridge.exe`.
   - Se o Windows mostrar "O Windows protegeu o computador", clique em **Mais informações > Executar assim mesmo**.
3. Na janela preta, quando perguntar, escolha **1 (SERVIDOR)**.
4. O Windows pergunta se quer **instalar o certificado** do StockBridge: confira que o nome começa com "StockBridge" e clique em **Sim**.
5. O **Firewall do Windows** pergunta se o StockBridge pode se comunicar: marque **Redes privadas** e clique em **Permitir acesso**.
   (Se a rede da empresa estiver marcada como "Pública" no Windows, mude para "Privada" em Configurações > Rede e Internet.)
6. O navegador abre sozinho na tela **Primeiro acesso**: crie o administrador (e-mail e senha de 10+ caracteres).
7. **Não feche a janela preta**: ela é o servidor. Para iniciar junto com o Windows, coloque um atalho do `StockBridge.exe`
   na pasta que abre com `Win + R` → `shell:startup`.

## 3. Nos outros computadores (terminais)
1. Copie o mesmo `StockBridge.exe` para uma pasta do computador.
2. Dê dois cliques. Ele procura o servidor sozinho e pergunta se este computador será **terminal**: responda **S**.
3. O Windows pergunta se quer instalar o certificado: **Sim**.
4. O navegador abre no sistema. Nas próximas vezes, é só abrir o `StockBridge.exe` (ou deixar a página nos favoritos,
   lembrando que o endereço pode mudar se o IP do servidor mudar; o `.exe` sempre acha o endereço certo).

Se não achar o servidor: confira se ele está ligado com a janela aberta, se os dois estão na mesma rede e se o firewall
do servidor liberou "Redes privadas". Dá para digitar o IP do servidor (aparece na janela do servidor).

**Celular ou PC sem o .exe:** no sistema, vá em Configurações > Rede, baixe o certificado e instale-o no aparelho; depois
acesse um dos endereços mostrados nessa tela.

## 4. Primeiros passos no sistema (administrador)
1. **Configurações > App do Mercado Livre**: cole o App ID, o Secret e a URI de redirecionamento do app criado no
   DevCenter do Mercado Livre (com a conta **dona dos anúncios**).
2. **Configurações > Backup**: escolha uma pasta fora deste computador (pendrive, HD externo, outro PC da rede).
3. **Conexão ML**: gere o link, autorize na conta, cole o endereço de retorno e conclua.
4. **Depósitos > Estantes**: cadastre as prateleiras (o código do QR é sugerido sozinho).
5. **Anúncios ML**: veja os anúncios da conta (SKU, vendas, visitas), selecione e clique em **Importar**. Cada anúncio
   vira um produto já vinculado, casando pelo SKU. Depois, em **Produtos**, preencha o **custo** de cada um.
6. **Onde está > Entrada em lote**: copie do Excel três colunas (SKU, estante, quantidade) e cole. Deixe "enviar ao
   Mercado Livre" desmarcado até lançar todas as estantes; depois use **Sincronizar** em Conexão ML.
7. **Rendimento**: faturamento, tarifas e lucro por período e por anúncio.
8. **Usuários**: crie um usuário para cada pessoa (operador = consulta e movimenta; administrador = tudo).

> Importar e vincular **não** mexe no estoque do Mercado Livre. O envio acontece a cada movimentação no sistema.
> Vendas feitas no Mercado Livre ainda **não** dão baixa sozinhas: registre a saída ao separar o pedido.

## 5. Backup e restauração
- O sistema faz **1 backup por dia** e guarda os 14 últimos (`dados\backups` ou a pasta escolhida). Também dá para
  clicar em **Fazer backup agora** em Configurações.
- **Para restaurar:** feche o StockBridge no servidor, copie o arquivo de backup escolhido (`stockbridge-AAAAMMDD-HHMMSS.db`)
  para `dados\stockbridge.db` (substituindo) e abra de novo. Apague antes, se existirem, `stockbridge.db-wal` e `stockbridge.db-shm`.
- **Trocar de computador servidor:** feche o StockBridge, copie a pasta `dados` inteira e o `.exe` para o novo computador
  e abra lá. Os terminais encontram o novo servidor sozinhos (como o certificado é o mesmo, não pedem nada).

## 6. Problemas comuns
| Sintoma | O que fazer |
|---|---|
| "A porta 3000 já está em uso" | O StockBridge já está aberto neste computador. Feche a outra janela. |
| Navegador mostra aviso de segurança | O certificado não foi instalado neste computador. Abra o `StockBridge.exe` de novo e clique em Sim, ou baixe-o em Configurações > Rede. |
| Terminal não acha o servidor | Servidor ligado? Mesma rede? Firewall liberou "Redes privadas"? Digite o IP mostrado na janela do servidor. |
| "Muitas tentativas" no login | 5 senhas erradas seguidas neste computador. Espere 15 minutos. |
| "Sessão encerrada" | A senha, o papel ou o acesso foi alterado pelo administrador. Entre de novo. |
| Mercado Livre: "precisa reconectar" | Gere um link novo em Mercado Livre e autorize de novo. |
| Anúncio com "Falhou" | Leia o motivo na própria tela. Anúncios com variações ainda não são atualizados pelo sistema. |
| Trocar o papel deste computador | Abra pelo Prompt: `StockBridge.exe --reconfigurar` |

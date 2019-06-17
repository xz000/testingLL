using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public class WeiQi : MonoBehaviour
{
    private int[,] Chou = new int[19, 19];
    private int[,] Lon = new int[19, 19];
    bool nowBlack = false;
    bool oneUnreal = false;
    public GameObject qizi;
    public WeiQiScore blackEaten;
    public WeiQiScore whiteEaten;
    public static Color PanColor;
    // Start is called before the first frame update
    void Start()
    {
        PanColor = GetComponent<SpriteRenderer>().color;
        chushihuaQi();
    }

    void Update()
    {
        if (Input.GetMouseButtonDown(0))
            CatchHim();
    }

    private bool canSet(WeiQiZi qizi)
    {
        if (qizi.srZhong.color != WeiQi.PanColor)
            return false;
        int color = 1;
        if (nowBlack)
            color = 2;
        int x = qizi.x + 9;
        int y = qizi.y + 9;
        for (int a = -1; a < 2; a += 2)
        {
            if (19 > x + a && x + a >= 0)
                if (Chou[x + a, y] != color)
                    return true;
            if (19 > y + a && y + a >= 0)
                if (Chou[x, y + a] != color)
                    return true;
        }
        return false;
    }

    public void setChou(WeiQiZi qizi, int color)
    {
        Chou[qizi.x + 9, qizi.y + 9] = color;
        switch (color)
        {
            case 1:
                qizi.goBlack();
                break;
            case 2:
                qizi.goWhite();
                break;
            case 0:
                qizi.goDie();
                break;
        }
    }

    private void jieLon()
    {
        int color = 1;
        if (nowBlack)
            color = 2;
        Lon = new int[19, 19];
        int n = 0;
        for (int i = 0; i < 19; i++)
        {
            for (int j = 0; j < 19; j++)
            {
                if (Lon[i, j] != 0)
                    continue;
                if (Chou[i, j] == color)
                {
                    n++;
                    if (lianZhu(i, j, n))
                        killNum(n);
                }
            }
        }
    }

    private void killNum(int n)
    {
        for (int i = 0; i < 19; i++)
        {
            for (int j = 0; j < 19; j++)
            {
                if (Lon[i, j] == n)
                    killSingle(i, j);
            }
        }
    }

    private void killSingle(int i, int j)
    {
        switch (Chou[i, j])
        {
            case 1:
                blackEaten.mePlusone();
                break;
            case 2:
                whiteEaten.mePlusone();
                break;
        }
        Chou[i, j] = 0;
        Collider2D hit = Physics2D.OverlapPoint(new Vector2(i - 9, j - 9));
        hit.GetComponent<WeiQiZi>().goDie();
    }

    private bool lianZhu(int i, int j, int n)
    {
        bool sha = true;
        Lon[i, j] = n;
        //Debug.Log(i + "," + j + "," + n);
        for (int a = -1; a < 2; a += 2)
        {
            if (19 > i + a && i + a >= 0)
                switch (Chou[i + a, j])
                {
                    case 0:
                        sha = false;
                        break;
                    default:
                        if (Chou[i + a, j] == Chou[i, j] && Lon[i + a, j] == 0)
                            sha = sha & lianZhu(i + a, j, n);
                        break;
                }
            if (19 > j + a && j + a >= 0)
                switch (Chou[i, j + a])
                {
                    case 0:
                        sha = false;
                        break;
                    default:
                        if (Chou[i, j + a] == Chou[i, j] && Lon[i, j + a] == 0)
                            sha = sha & lianZhu(i, j + a, n);
                        break;
                }
        }
        return sha;
    }

    void chushihuaQi()
    {
        for (int i = -9; i < 10; i++)
        {
            for (int j = -9; j < 10; j++)
            {
                GameObject nq = GameObject.Instantiate(qizi, new Vector3(i, j, 0), Quaternion.identity);
                nq.GetComponent<WeiQiZi>().setXY(i, j);
            }
        }
    }

    public void ChangeColor()
    {
        nowBlack = !nowBlack;
    }

    public void ToBlack()
    {
        nowBlack = true;
    }

    public void ToWhite()
    {
        nowBlack = false;
    }

    void CatchHim()
    {
        Collider2D hit = Physics2D.OverlapPoint(Camera.main.ScreenToWorldPoint(Input.mousePosition));
        if (!hit)
            return;
        if (hit.GetComponent<WeiQiZi>())
        {
            if (!canSet(hit.GetComponent<WeiQiZi>()))
                return;
            if (nowBlack)
                setChou(hit.GetComponent<WeiQiZi>(), 1);
            else
                setChou(hit.GetComponent<WeiQiZi>(), 2);
            oneUnreal = false;
            jieLon();
            ChangeColor();
            jieLon();
        }
        if (hit.GetComponent<WeiQiControl>())
        {
            hit.GetComponent<WeiQiControl>().pickMe();
        }
        if (hit.GetComponent<WeiQiScore>() && hit.GetComponent<WeiQiScore>().IBlack == nowBlack)
        {
            hit.GetComponent<WeiQiScore>().mePlusone();
            ChangeColor();
            if (oneUnreal)
                endGame();
            oneUnreal = true;
        }
    }

    void endGame()
    {
        Debug.Log("Fin");
    }
}
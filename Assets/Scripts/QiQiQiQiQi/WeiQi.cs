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

    private bool jieLon(int color)
    {
        Lon = new int[19, 19];
        int n = 0;
        int a = 0;
        for (int i = 0; i < 19; i++)
        {
            for (int j = 0; j < 19; j++)
            {
                if (Lon[i, j] != 0)
                    continue;
                if (Chou[i, j] == color)
                {
                    n++;
                    bool notme = (color == 1 ^ nowBlack);
                    if (lianZhu(i, j, n))
                        a += killNum(n);
                }
            }
        }
        if (color == 1 ^ nowBlack)
            return false;
        if (a == 1)
        {
            doUnreal();
            return true;
        }
        return false;
    }

    private int killNum(int n)
    {
        int a = 0;
        for (int i = 0; i < 19; i++)
        {
            for (int j = 0; j < 19; j++)
            {
                if (Lon[i, j] == n)
                {
                    killSingle(i, j);
                    a++;
                }
            }
        }
        return a;
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
            if (hit.GetComponent<WeiQiZi>().srZhong.color != WeiQi.PanColor)
                return;
            int c = nowBlack ? 1 : 2;
            setChou(hit.GetComponent<WeiQiZi>(), c);
            jieLon(3 - c);
            oneUnreal = jieLon(c);
            ChangeColor();
        }
        if (hit.GetComponent<WeiQiControl>())
        {
            hit.GetComponent<WeiQiControl>().pickMe();
        }
        if (hit.GetComponent<WeiQiScore>() && hit.GetComponent<WeiQiScore>().IBlack == nowBlack)
        {
            hit.GetComponent<WeiQiScore>().mePlusone();
            doUnreal();
            ChangeColor();
        }
    }

    void doUnreal()
    {
        if (oneUnreal)
            endGame();
        oneUnreal = true;
    }

    void endGame()
    {
        Debug.Log("Fin");
    }
}